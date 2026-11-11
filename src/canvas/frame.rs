use super::{DEFAULT_MAX_ZOOM, DEFAULT_MIN_ZOOM};
use gpui_kit::{
    AnyElement, App, Bounds, ContentMask, Element, ElementId, GlobalElementId, InspectorElementId,
    IntoElement, LayoutId, Pixels, Point, Style, Window, point, px, relative, size,
};

/// 唯一相机控制容器：维护 `zoom` / `pan`，在 1x 世界空间调度子 [`super::CanvasElement`]。
pub struct CanvasFrame {
    zoom: f32,
    pan: Point<Pixels>,
    children: Vec<AnyElement>,
}

impl CanvasFrame {
    pub fn new(zoom: f32, pan: Point<Pixels>) -> Self {
        Self {
            zoom: zoom.clamp(DEFAULT_MIN_ZOOM, DEFAULT_MAX_ZOOM),
            pan,
            children: Vec::new(),
        }
    }

    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.children.push(child.into_any_element());
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = impl IntoElement>) -> Self {
        self.children.extend(children.into_iter().map(|c| c.into_any_element()));
        self
    }
}

impl IntoElement for CanvasFrame {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for CanvasFrame {
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
        for child in &mut self.children {
            child.request_layout(window, cx);
        }

        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        let layout_id = window.request_layout(style, [], cx);

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
        let pivot = bounds.origin;
        let zoom = self.zoom;
        let pan = self.pan;

        window.with_canvas_transform(pivot, zoom, pan, |window| {
            for child in &mut self.children {
                child.prepaint_at(pivot, window, cx);
            }
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
        let pivot = bounds.origin;
        // 画布整体建立一个足够覆盖全部世界坐标图元的全局图层边界，
        // 既与父级视口相交以建立高于父级背景的 DrawOrder，又覆盖所有世界坐标下的子元素。
        let canvas_bounds = Bounds::new(
            point(pivot.x - px(500_000.), pivot.y - px(500_000.)),
            size(px(1_000_000.), px(1_000_000.)),
        );

        window.with_scaled_paint(pivot, self.zoom, self.pan, |window| {
            window.with_unclipped_content_mask(ContentMask { bounds: canvas_bounds }, |window| {
                window.paint_layer(canvas_bounds, |window| {
                    for child in &mut self.children {
                        child.paint(window, cx);
                    }
                });
            });
        });
    }
}
