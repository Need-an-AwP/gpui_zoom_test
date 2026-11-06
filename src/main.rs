use gpui::{
    AnyElement, App, AvailableSpace, Bounds, Context, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Pixels, ScrollDelta, ScrollEvent, Size, Style,
    Window, WindowBounds, WindowOptions,
};
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::text::TextView;
use gpui_kit::component::{blue, green, red, white};
use gpui_kit::*;

const MD_SAMPLE: &str = r#"# Markdown 示例

一段普通文字，包含 **粗体**、*斜体* 和 `inline code`。

## 列表

- 第一项
- 第二项
  - 嵌套项

## 代码块

```rust
fn main() {
    println!("Hello, gpui-kit!");
}
```

> 引用块：TextView 默认配置渲染 Markdown。
"#;

/// 卡片在 zoom = 1.0 时的固定逻辑宽度（默认 rem，不再放大排版）
const CARD_WIDTH: f32 = 380.0;
const MIN_ZOOM: f32 = 0.1;
const MAX_ZOOM: f32 = 1.0;

/// 固定尺寸排版绘制，paint 后通过 Scene 浮点缩放适配当前 zoom
struct ScaledPaintZoom {
    zoom: f32,
    child: AnyElement,
}

impl ScaledPaintZoom {
    fn new(zoom: f32, child: impl IntoElement) -> Self {
        Self {
            zoom: zoom.clamp(MIN_ZOOM, MAX_ZOOM),
            child: child.into_any_element(),
        }
    }
}

impl IntoElement for ScaledPaintZoom {
    type Element = Self;
    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for ScaledPaintZoom {
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
        let ref_size = self.child.layout_as_root(
            size(AvailableSpace::MinContent, AvailableSpace::MinContent),
            window,
            cx,
        );

        let k = self.zoom;
        let mut style = Style::default();
        style.size.width = px(ref_size.width.as_f32() * k).into();
        style.size.height = px(ref_size.height.as_f32() * k).into();
        let layout_id = window.request_layout(style, [], cx);

        (layout_id, ref_size)
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _ref_size: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        // prepaint 的可见性裁剪也要在未缩放坐标系下进行，否则右/下部分会被剔除
        let zoom = self.zoom;
        window.with_scaled_content_mask(bounds.origin, zoom, |window| {
            self.child.prepaint_at(bounds.origin, window, cx);
        });
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _ref_size: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_scaled_paint(bounds.origin, self.zoom, |window| {
            self.child.paint(window, cx);
        });
    }
}

fn card_column(button_color: gpui::Hsla, child: impl IntoElement) -> impl IntoElement {
    div()
        .w(px(CARD_WIDTH))
        .size_full()
        .flex()
        .flex_col()
        .gap_3()
        .p_4()
        .rounded_md()
        .bg(button_color.opacity(0.5))
        .border_5()
        .border_color(red(500))
        .child(child)
}

struct HelloWorld {
    zoom: f32,
    /// true 为红色，false 为蓝色
    button_red: bool,
    note: Entity<TextareaState>,
}

impl Render for HelloWorld {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let zoom = self.zoom;
        let button_color = if self.button_red { red(500) } else { blue(500) };
        let button_style = ButtonCustomVariant::new(cx)
            .color(button_color)
            .foreground(white())
            .hover(button_color)
            .active(button_color);
        div()
            .flex()
            .flex_col()
            .size_full()
            .items_center()
            .justify_center()
            .bg(green(500).opacity(0.5))
            .on_scroll(cx.listener(|this, event: &ScrollEvent, _, cx| {
                println!("scroll event: {:?}", event);
                let delta_y = match event.delta {
                    ScrollDelta::Pixels(p) => p.y.as_f32(),
                    ScrollDelta::Lines(l) => {
                        if event.modifiers.control {
                            l.y * 24.0
                        } else {
                            l.y / 10.0 * 24.0 // 10x slower if hold ctrl
                        }
                    } // 24px line height
                };
                let factor = (1.0 + delta_y * 0.002).clamp(0.8, 1.25); // 每 1px 垂直增量 ≈ 0.2% 的乘性缩放（0.002）
                this.zoom = (this.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
                cx.notify();
            }))
            .child(ScaledPaintZoom::new(
                zoom,
                div()
                    .grid()
                    .grid_cols_min_content(2)
                    .gap_6()
                    .child(card_column(
                        button_color,
                        div()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .child(TextView::markdown("md-demo", MD_SAMPLE))
                            .child(
                                Button::new("color-toggle")
                                    .custom(button_style)
                                    .label("切换颜色")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.button_red = !this.button_red;
                                        cx.notify();
                                    })),
                            ),
                    ))
                    .child(card_column(
                        button_color,
                        div()
                            .flex_1()
                            .w(px(CARD_WIDTH / 2.))
                            .flex()
                            .flex_col()
                            .child(Textarea::new(&self.note).h_full()),
                    )),
            ))
    }
}

fn main() {
    application().with_assets(assets::Assets).run(|cx| {
        init(cx);

        let bounds = Bounds::centered(None, size(px(900.), px(650.)), cx);
        open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            cx,
            |window, cx| {
                let note =
                    cx.new(|cx| TextareaState::new(window, cx).placeholder("example textarea"));
                cx.new(|_| HelloWorld {
                    zoom: 1.0,
                    button_red: true,
                    note,
                })
            },
        )
        .expect("Failed to open window");
    });
}
