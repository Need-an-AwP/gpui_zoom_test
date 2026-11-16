use gpui_kit::{Pixels, Point, Size, Window, point, px};
#[cfg(not(target_family = "wasm"))]
use std::time::Instant;
#[cfg(target_family = "wasm")]
use web_time::Instant;

use super::{DEFAULT_MAX_ZOOM, DEFAULT_MIN_ZOOM};

/// 硬编码的受控平移/缩放动画时长（毫秒）。
const PAN_ANIMATION_DURATION_MS: f32 = 450.0;
const ZOOM_ANIMATION_DURATION_MS: f32 = 450.0;

/// 1 倍缩放时强制将渲染用的 `pan` 对齐到最近设备像素，避免亚像素平移导致文本模糊。
const SNAP_PAN_TO_DEVICE_PIXELS_AT_1X: bool = true;

fn ease_in_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

struct PanAnimation {
    start_time: Instant,
    from: Point<Pixels>,
    to: Point<Pixels>,
}

struct ZoomAnimation {
    start_time: Instant,
    from_zoom: f32,
    to_zoom: f32,
    anchor: Point<Pixels>,
    anchor_world: Point<Pixels>,
}

/// 单帧相机动画推进结果。
#[derive(Clone, Copy, Debug, Default)]
pub struct CameraTick {
    /// 是否仍有动画在进行，需要继续请求下一帧。
    pub animating: bool,
    /// 本帧是否修改了 `zoom`（含最后一帧吸附到目标值）。
    pub zoom_changed: bool,
}

/// 2D 画布相机状态，维护缩放比例与屏幕平移量，并提供世界坐标与屏幕坐标的换算。
pub struct Camera {
    zoom: f32,
    pan: Point<Pixels>,
    active_pan: Option<PanAnimation>,
    active_zoom: Option<ZoomAnimation>,
}

impl Camera {
    pub fn new(zoom: f32, pan: Point<Pixels>) -> Self {
        Self {
            zoom: zoom.clamp(DEFAULT_MIN_ZOOM, DEFAULT_MAX_ZOOM),
            pan,
            active_pan: None,
            active_zoom: None,
        }
    }

    pub fn zoom(&self) -> f32 {
        self.zoom
    }
    /// 返回用于渲染的 `pan`；当启用强制对齐且 `zoom == 1.0` 时，对齐到最近设备像素。
    pub fn pan(&self, window: &Window) -> Point<Pixels> {
        if SNAP_PAN_TO_DEVICE_PIXELS_AT_1X && (self.zoom - 1.0).abs() <= f32::EPSILON {
            window.pixel_snap_point(self.pan)
        } else {
            self.pan
        }
    }

    /// 世界坐标换算为屏幕坐标：`screen = world * zoom + pan`。
    #[allow(dead_code)]
    pub fn world_to_screen(&self, world_pt: Point<Pixels>) -> Point<Pixels> {
        world_pt * self.zoom + self.pan
    }

    /// 屏幕坐标换算为世界坐标：`world = (screen - pan) / zoom`。
    #[allow(dead_code)]
    pub fn screen_to_world(&self, screen_pt: Point<Pixels>) -> Point<Pixels> {
        (screen_pt - self.pan) / self.zoom
    }

    /// 计算将指定世界坐标点对齐到指定屏幕坐标点所需的 `pan`。
    pub fn pan_for_world_at_screen(
        &self,
        world_pt: Point<Pixels>,
        screen_pt: Point<Pixels>,
    ) -> Point<Pixels> {
        screen_pt - world_pt * self.zoom
    }

    /// 平移相机，使指定世界坐标点落在视口中心（保持当前缩放不变）。
    #[allow(dead_code)]
    pub fn center_on_world_point(&mut self, world_pt: Point<Pixels>, viewport_size: Size<Pixels>) {
        self.pan_to(self.pan_for_world_at_screen(world_pt, viewport_size.center()));
    }

    /// 以指定屏幕锚点为中心即时缩放到目标比例。
    pub fn zoom_to(&mut self, zoom: f32, anchor: Point<Pixels>) {
        self.active_pan = None;
        self.active_zoom = None;
        let k0 = self.zoom;
        let k1 = zoom.clamp(DEFAULT_MIN_ZOOM, DEFAULT_MAX_ZOOM);
        if k1 == k0 {
            return;
        }
        let local = point((anchor.x - self.pan.x) / k0, (anchor.y - self.pan.y) / k0);
        self.pan = point(anchor.x - local.x * k1, anchor.y - local.y * k1);
        self.zoom = k1;
    }

    /// 即时平移到目标 `pan`。
    pub fn pan_to(&mut self, pan: Point<Pixels>) {
        self.active_pan = None;
        self.active_zoom = None;
        self.pan = pan;
    }

    /// 启动受控曲线平移（硬编码 450ms + ease-in-out cubic，只做移动不做缩放）。
    pub fn animate_pan_to(&mut self, target: Point<Pixels>) {
        self.active_zoom = None;
        if self.pan == target {
            self.active_pan = None;
            return;
        }
        self.active_pan = Some(PanAnimation {
            start_time: Instant::now(),
            from: self.pan,
            to: target,
        });
    }

    /// 启动受控曲线缩放（硬编码 450ms + ease-in-out cubic，对数空间插值并锁定屏幕锚点）。
    pub fn animate_zoom_to(&mut self, target_zoom: f32, anchor: Point<Pixels>) {
        self.active_pan = None;
        let target = target_zoom.clamp(DEFAULT_MIN_ZOOM, DEFAULT_MAX_ZOOM);
        if (target - self.zoom).abs() <= f32::EPSILON {
            self.active_zoom = None;
            return;
        }
        let anchor_world = self.screen_to_world(anchor);
        self.active_zoom = Some(ZoomAnimation {
            start_time: Instant::now(),
            from_zoom: self.zoom,
            to_zoom: target,
            anchor,
            anchor_world,
        });
    }

    /// 受控曲线平移相机，使指定世界坐标点落在视口中心（保持当前缩放不变）。
    pub fn animate_center_on_world_point(
        &mut self,
        world_pt: Point<Pixels>,
        viewport_size: Size<Pixels>,
    ) {
        self.animate_pan_to(self.pan_for_world_at_screen(world_pt, viewport_size.center()));
    }

    /// 推进相机动画（平移与缩放）。
    pub fn tick(&mut self, now: Instant) -> CameraTick {
        let pan_animating = self.tick_pan(now);
        let (zoom_animating, zoom_changed) = self.tick_zoom(now);
        CameraTick {
            animating: pan_animating || zoom_animating,
            zoom_changed,
        }
    }

    fn tick_pan(&mut self, now: Instant) -> bool {
        let Some(anim) = &self.active_pan else {
            return false;
        };
        let elapsed_ms = now.saturating_duration_since(anim.start_time).as_secs_f32() * 1000.0;
        let t = (elapsed_ms / PAN_ANIMATION_DURATION_MS).clamp(0.0, 1.0);
        let u = ease_in_out_cubic(t);

        self.pan = point(
            px(anim.from.x.as_f32() + (anim.to.x.as_f32() - anim.from.x.as_f32()) * u),
            px(anim.from.y.as_f32() + (anim.to.y.as_f32() - anim.from.y.as_f32()) * u),
        );

        if t >= 1.0 {
            self.pan = anim.to;
            self.active_pan = None;
            false
        } else {
            true
        }
    }

    fn tick_zoom(&mut self, now: Instant) -> (bool, bool) {
        let Some(anim) = &self.active_zoom else {
            return (false, false);
        };
        let elapsed_ms = now.saturating_duration_since(anim.start_time).as_secs_f32() * 1000.0;
        let t = (elapsed_ms / ZOOM_ANIMATION_DURATION_MS).clamp(0.0, 1.0);
        let u = ease_in_out_cubic(t);

        // 在对数空间插值缩放倍率，使视觉缩放速度与曲线斜率一致，并保持锚点不动。
        let next_zoom = if t >= 1.0 {
            anim.to_zoom
        } else {
            anim.from_zoom * (anim.to_zoom / anim.from_zoom).powf(u)
        };
        self.zoom = next_zoom;
        self.pan = anim.anchor - anim.anchor_world * next_zoom;

        if t >= 1.0 {
            self.active_zoom = None;
            (false, true)
        } else {
            (true, true)
        }
    }
}
