use std::rc::Rc;

use gpui_kit::{
    App, Bounds, InteractiveElement as _, IntoElement, MouseButton, MouseDownEvent, ParentElement as _,
    Pixels, Point, Size, Styled as _, Window, div, px,
};

/// 缩放手柄所在的边。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeAxis {
    /// 右侧边：只改宽度。
    Right,
    /// 下侧边：只改高度。
    Bottom,
}

type ResizeStart = Rc<dyn Fn(ResizeAxis, &MouseDownEvent, &mut Window, &mut App)>;

/// 手柄热区的屏幕像素厚度（不随 zoom 变化）。
const HANDLE_THICKNESS: f32 = 8.;

/// 渲染在 CanvasFrame 同级、处于 1x 屏幕空间的卡片高亮选中框/覆盖层。
///
/// 接收卡片在世界坐标系下的位置、尺寸以及当前相机的 zoom 与 pan，
/// 在屏幕坐标系下计算出精确定位，保证边框线宽等视觉样式在缩放与拖动过程中恒定。
pub struct SelectionOverlay {
    world_pos: Point<Pixels>,
    world_size: Size<Pixels>,
    zoom: f32,
    pan: Point<Pixels>,
    on_resize_start: Option<ResizeStart>,
}

impl SelectionOverlay {
    pub fn new(
        world_pos: Point<Pixels>,
        world_size: Size<Pixels>,
        zoom: f32,
        pan: Point<Pixels>,
    ) -> Self {
        Self {
            world_pos,
            world_size,
            zoom,
            pan,
            on_resize_start: None,
        }
    }

    /// 在右侧/下侧手柄上按下左键时触发。后续的移动与释放由调用方处理。
    pub fn on_resize_start(
        mut self,
        listener: impl Fn(ResizeAxis, &MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_resize_start = Some(Rc::new(listener));
        self
    }

    /// 根据世界坐标位置、尺寸和相机状态计算在屏幕上的 Bounds。
    pub fn screen_bounds(&self) -> Bounds<Pixels> {
        let screen_x = self.world_pos.x * self.zoom + self.pan.x;
        let screen_y = self.world_pos.y * self.zoom + self.pan.y;
        let screen_w = self.world_size.width * self.zoom;
        let screen_h = self.world_size.height * self.zoom;

        Bounds::new(
            gpui_kit::point(screen_x, screen_y),
            gpui_kit::size(screen_w, screen_h),
        )
    }
}

impl IntoElement for SelectionOverlay {
    type Element = gpui_kit::Div;

    fn into_element(self) -> Self::Element {
        let bounds = self.screen_bounds();
        let half = px(HANDLE_THICKNESS / 2.);

        let handle = |axis: ResizeAxis| {
            let on_start = self.on_resize_start.clone();
            let base = div()
                .absolute()
                .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                    cx.stop_propagation();
                    if let Some(on_start) = on_start.as_ref() {
                        on_start(axis, event, window, cx);
                    }
                });
            match axis {
                ResizeAxis::Right => base
                    .right(-half)
                    .top_0()
                    .bottom_0()
                    .w(px(HANDLE_THICKNESS))
                    .cursor_ew_resize(),
                ResizeAxis::Bottom => base
                    .bottom(-half)
                    .left_0()
                    .right_0()
                    .h(px(HANDLE_THICKNESS))
                    .cursor_ns_resize(),
            }
        };

        div()
            .absolute()
            .left(bounds.origin.x)
            .top(bounds.origin.y)
            .w(bounds.size.width)
            .h(bounds.size.height)
            .border_2()
            .border_color(gpui_kit::component::blue(500))
            .child(handle(ResizeAxis::Right))
            .child(handle(ResizeAxis::Bottom))
    }
}
