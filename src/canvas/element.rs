use gpui_kit::{
    AnyElement, App, AvailableSpace, Bounds, ContentMask, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Pixels, Point, Size, Style, Window, point,
};

/// 在 1x 世界坐标下绝对定位子元素，并提供随动 `ContentMask` 以稳定 View 缓存。
///
/// 尺寸来自子树一次 [`AnyElement::layout_as_root`]，不再接收外部写死的 size。
pub struct CanvasElement {
    world_pos: Point<Pixels>,
    child: AnyElement,
    on_measured: Option<Box<dyn Fn(Size<Pixels>, &mut App) -> bool>>,
}

impl CanvasElement {
    pub fn new(world_pos: Point<Pixels>, child: impl IntoElement) -> Self {
        Self {
            world_pos,
            child: child.into_any_element(),
            on_measured: None,
        }
    }

    /// 布局结束后收到 Taffy 的 border-box。返回 `true` 表示尺寸已写入，需要再画一帧。
    pub fn on_measured(
        mut self,
        on_measured: impl Fn(Size<Pixels>, &mut App) -> bool + 'static,
    ) -> Self {
        self.on_measured = Some(Box::new(on_measured));
        self
    }

    fn world_bounds(&self, frame_origin: Point<Pixels>, measured: Size<Pixels>) -> Bounds<Pixels> {
        Bounds::new(
            point(
                frame_origin.x + self.world_pos.x,
                frame_origin.y + self.world_pos.y,
            ),
            measured,
        )
    }
}

impl IntoElement for CanvasElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for CanvasElement {
    type RequestLayoutState = Size<Pixels>;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        // 子节点样式自带定宽，高度由内容决定。这里不再传入 Definite size。
        let measured = self.child.layout_as_root(AvailableSpace::min_size(), window, cx);
        if let Some(on_measured) = self.on_measured.as_ref()
            && on_measured(measured, cx)
        {
            // 布局阶段里 `notify` 不会再排一帧，这里把缓存样式的安装推迟到下一帧。
            window.request_animation_frame();
        }

        let layout_id = window.request_layout(Style::default(), [], cx);
        (layout_id, measured)
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        measured: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let world_bounds = self.world_bounds(bounds.origin, *measured);
        let mask = ContentMask { bounds: world_bounds };

        window.with_unclipped_content_mask(mask, |window| {
            self.child.prepaint_at(world_bounds.origin, window, cx);
        });
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        measured: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let world_bounds = self.world_bounds(bounds.origin, *measured);
        let mask = ContentMask { bounds: world_bounds };

        window.with_unclipped_content_mask(mask, |window| {
            window.paint_layer(world_bounds, |window| {
                self.child.paint(window, cx);
            });
        });
    }
}
