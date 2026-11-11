use gpui_kit::{
    AnyElement, App, AvailableSpace, Bounds, ContentMask, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Pixels, Point, Size, Style, Window, point, size,
};

/// 在 1x 世界坐标下绝对定位子元素，并提供随动 `ContentMask` 以稳定 View 缓存。
pub struct CanvasElement {
    world_pos: Point<Pixels>,
    size: Size<Pixels>,
    child: AnyElement,
}

impl CanvasElement {
    pub fn new(world_pos: Point<Pixels>, size: Size<Pixels>, child: impl IntoElement) -> Self {
        Self {
            world_pos,
            size,
            child: child.into_any_element(),
        }
    }

    fn world_bounds(&self, frame_origin: Point<Pixels>) -> Bounds<Pixels> {
        Bounds::new(
            point(
                frame_origin.x + self.world_pos.x,
                frame_origin.y + self.world_pos.y,
            ),
            self.size,
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
    type RequestLayoutState = ();
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
        self.child.layout_as_root(
            size(
                AvailableSpace::Definite(self.size.width),
                AvailableSpace::Definite(self.size.height),
            ),
            window,
            cx,
        );

        let layout_id = window.request_layout(Style::default(), [], cx);
        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _state: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let world_bounds = self.world_bounds(bounds.origin);
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
        _state: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let world_bounds = self.world_bounds(bounds.origin);
        let mask = ContentMask { bounds: world_bounds };

        window.with_unclipped_content_mask(mask, |window| {
            window.paint_layer(world_bounds, |window| {
                self.child.paint(window, cx);
            });
        });
    }
}
