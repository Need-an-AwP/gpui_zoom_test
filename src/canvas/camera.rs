use gpui_kit::{Point, Pixels, Size, point, px};

use super::{DEFAULT_MAX_ZOOM, DEFAULT_MIN_ZOOM};

/// 硬编码的受控平移动画时长（毫秒）。
const PAN_ANIMATION_DURATION_MS: f32 = 450.0;

fn ease_in_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

struct PanAnimation {
    start_time: std::time::Instant,
    from: Point<Pixels>,
    to: Point<Pixels>,
}

/// 2D 画布相机状态，维护缩放比例与屏幕平移量，并提供世界坐标与屏幕坐标的换算。
pub struct Camera {
    zoom: f32,
    pan: Point<Pixels>,
    active_pan: Option<PanAnimation>,
}

impl Camera {
    pub fn new(zoom: f32, pan: Point<Pixels>) -> Self {
        Self {
            zoom: zoom.clamp(DEFAULT_MIN_ZOOM, DEFAULT_MAX_ZOOM),
            pan,
            active_pan: None,
        }
    }

    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    pub fn pan(&self) -> Point<Pixels> {
        self.pan
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
        self.pan = pan;
    }

    /// 启动受控曲线平移（硬编码 450ms + ease-in-out cubic，只做移动不做缩放）。
    pub fn animate_pan_to(&mut self, target: Point<Pixels>) {
        if self.pan == target {
            self.active_pan = None;
            return;
        }
        self.active_pan = Some(PanAnimation {
            start_time: std::time::Instant::now(),
            from: self.pan,
            to: target,
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

    /// 推进平移动画；返回 `true` 表示仍在运动中，需要继续请求下一帧。
    pub fn tick_pan(&mut self, now: std::time::Instant) -> bool {
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
}
