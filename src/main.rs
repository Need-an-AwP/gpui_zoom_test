use gpui::{
    AnyElement, App, AvailableSpace, Bounds, Context, CursorStyle, Element, ElementId,
    GlobalElementId, InspectorElementId, IntoElement, LayoutId, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, Point, ScrollDelta, ScrollEvent, Size, Style,
    TitlebarOptions, Window, WindowBounds, WindowOptions, point, px,
};
use gpui_fps::fps_monitor;
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::text::TextView;
use gpui_kit::component::{blue, green, red, white};
use gpui_kit::prelude::FluentBuilder as _;
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
const COLS: u16 = 10;
const ROWS: u16 = 10;

/// 固定尺寸排版绘制，paint 后通过 Scene 浮点缩放适配当前 zoom
struct ScaledPaintZoom {
    zoom: f32,
    pan: Point<Pixels>,
    child: AnyElement,
}

impl ScaledPaintZoom {
    fn new(zoom: f32, pan: Point<Pixels>, child: impl IntoElement) -> Self {
        Self {
            zoom: zoom.clamp(MIN_ZOOM, MAX_ZOOM),
            pan,
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
        let origin = bounds.origin + self.pan;
        let zoom = self.zoom;
        window.with_scaled_content_mask(origin, zoom, |window| {
            self.child.prepaint_at(origin, window, cx);
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
        let origin = bounds.origin + self.pan;
        window.with_scaled_paint(origin, self.zoom, |window| {
            self.child.paint(window, cx);
        });
    }
}

fn card_column(button_color: gpui::Hsla, child: impl IntoElement) -> impl IntoElement {
    div()
        .w(px(CARD_WIDTH))
        .h_full()
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

fn markdown_card(
    idx: usize,
    button_color: gpui::Hsla,
    cx: &mut Context<HelloWorld>,
) -> impl IntoElement {
    let button_style = ButtonCustomVariant::new(cx)
        .color(button_color)
        .foreground(white())
        .hover(button_color)
        .active(button_color);

    card_column(
        button_color,
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(TextView::markdown(format!("md-demo-{}", idx), MD_SAMPLE))
            .child(
                Button::new(format!("color-toggle-{}", idx))
                    .custom(button_style)
                    .label("切换颜色")
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.button_red = !this.button_red;
                        cx.notify();
                    })),
            ),
    )
}

fn textarea_card(button_color: gpui::Hsla, note: &Entity<TextareaState>) -> impl IntoElement {
    card_column(
        button_color,
        div()
            .flex_1()
            .flex()
            .flex_col()
            .min_h(px(260.))
            .child(Textarea::new(note).h_full()),
    )
}

fn example_grid_cards(
    cols: u16,
    rows: u16,
    button_color: gpui::Hsla,
    notes: &[Entity<TextareaState>],
    cx: &mut Context<HelloWorld>,
) -> impl IntoElement {
    div()
        .grid()
        .grid_cols_min_content(cols)
        .gap_6()
        .children((0..cols * rows).map(|i| {
            if i % 2 == 0 {
                markdown_card(i.into(), button_color, cx).into_any_element()
            } else {
                textarea_card(button_color, &notes[(i / 2) as usize]).into_any_element()
            }
        }))
}

struct HelloWorld {
    zoom: f32,
    pan: Point<Pixels>,
    drag_start: Option<(Point<Pixels>, Point<Pixels>)>,
    /// true 为红色，false 为蓝色
    button_red: bool,
    notes: Vec<Entity<TextareaState>>,
}

impl Render for HelloWorld {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let zoom = self.zoom;
        let pan = self.pan;
        let is_dragging = self.drag_start.is_some();
        let button_color = if self.button_red { red(500) } else { blue(500) };

        div()
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(green(500).opacity(0.5))
            .when(is_dragging, |this| this.cursor(CursorStyle::Crosshair))
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    this.drag_start = Some((event.position, this.pan));
                    cx.notify();
                }),
            )
            .on_mouse_up(
                MouseButton::Middle,
                cx.listener(|this, _event: &MouseUpEvent, _, cx| {
                    this.drag_start = None;
                    cx.notify();
                }),
            )
            .on_mouse_up_out(
                MouseButton::Middle,
                cx.listener(|this, _event: &MouseUpEvent, _, cx| {
                    this.drag_start = None;
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if let Some((start_pos, start_pan)) = this.drag_start {
                    if event.pressed_button == Some(MouseButton::Middle) {
                        this.pan = start_pan + (event.position - start_pos);
                        cx.notify();
                    } else {
                        this.drag_start = None;
                        cx.notify();
                    }
                }
            }))
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
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .child(ScaledPaintZoom::new(
                        zoom,
                        pan,
                        example_grid_cards(COLS, ROWS, button_color, &self.notes, cx),
                    )),
            )
            .child(
                div()
                    .absolute()
                    .top_0()
                    .right_0()
                    .child(fps_monitor(window, cx)),
            )
    }
}

fn main() {
    application().with_assets(assets::Assets).run(|cx| {
        init(cx);

        let bounds = Bounds::centered(None, size(px(900.), px(650.)), cx);
        open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("zoom test".into()),
                    ..TitlebarOptions::default()
                }),
                ..Default::default()
            },
            cx,
            |window, cx| {
                let notes = (1..=ROWS * COLS)
                    .map(|i| {
                        cx.new(|cx| {
                            TextareaState::new(window, cx)
                                .placeholder(format!("example textarea {}", i))
                        })
                    })
                    .collect();

                cx.new(|_| HelloWorld {
                    zoom: 1.0,
                    pan: point(px(40.), px(40.)),
                    drag_start: None,
                    button_red: true,
                    notes,
                })
            },
        )
        .expect("Failed to open window");
    });
}
