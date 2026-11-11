use gpui::{
    AnyElement, App, AvailableSpace, Bounds, ContentMask, Context, CursorStyle, Element,
    ElementId, GlobalElementId, InspectorElementId, IntoElement, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, ScrollDelta, ScrollEvent, Size,
    Style, Subscription, TitlebarOptions, Window, WindowBounds, WindowOptions, point, px,
};
use gpui_fps::fps_monitor;
use gpui_kit::component::Disableable;
use gpui_kit::component::Sizable;
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::component::text::TextView;
use gpui_kit::component::{blue, green, red, white};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

#[allow(dead_code)]
mod canvas;

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
    ref_size: Size<Pixels>,
    child: AnyElement,
}

impl ScaledPaintZoom {
    fn new(zoom: f32, pan: Point<Pixels>, ref_size: Size<Pixels>, child: impl IntoElement) -> Self {
        Self {
            zoom: zoom.clamp(MIN_ZOOM, MAX_ZOOM),
            pan,
            ref_size,
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
        let canvas_size = self.ref_size;
        self.child.layout_as_root(
            size(
                AvailableSpace::Definite(canvas_size.width),
                AvailableSpace::Definite(canvas_size.height),
            ),
            window,
            cx,
        );

        let k = self.zoom;
        let mut style = Style::default();
        style.size.width = px(canvas_size.width.as_f32() * k).into();
        style.size.height = px(canvas_size.height.as_f32() * k).into();
        let layout_id = window.request_layout(style, [], cx);

        (layout_id, canvas_size)
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
        let zoom = self.zoom;
        let pan = self.pan;
        let world_bounds = Bounds::new(bounds.origin, self.ref_size);

        window.with_canvas_transform(bounds.origin, zoom, pan, |window| {
            window.with_unclipped_content_mask(ContentMask { bounds: world_bounds }, |window| {
                self.child.prepaint_at(world_bounds.origin, window, cx);
            });
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
        let world_bounds = Bounds::new(bounds.origin, self.ref_size);
        window.with_scaled_paint(bounds.origin, self.zoom, self.pan, |window| {
            window.with_unclipped_content_mask(ContentMask { bounds: world_bounds }, |window| {
                window.paint_layer(world_bounds, |window| {
                    self.child.paint(window, cx);
                });
            });
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
    cx: &mut Context<CardsCanvas>,
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

struct CardsCanvas {
    button_red: bool,
    notes: Vec<Entity<TextareaState>>,
}

impl CardsCanvas {
    fn new(notes: Vec<Entity<TextareaState>>) -> Self {
        Self {
            button_red: true,
            notes,
        }
    }
}

impl Render for CardsCanvas {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let button_color = if self.button_red { red(500) } else { blue(500) };
        div()
            .grid()
            .grid_cols_min_content(COLS)
            .gap_6()
            .children((0..COLS * ROWS).map(|i| {
                // if i % 2 == 0 {
                //     markdown_card(i.into(), button_color, cx).into_any_element()
                // } else {
                //     textarea_card(button_color, &self.notes[(i / 2) as usize]).into_any_element()
                // }
                markdown_card(i.into(), button_color, cx).into_any_element()
            }))
    }
}

/// 画布左上角为枢轴：屏幕点 = 内容点 × zoom + pan
struct Camera {
    zoom: f32,
    pan: Point<Pixels>,
}

impl Camera {
    fn new(zoom: f32, pan: Point<Pixels>) -> Self {
        Self {
            zoom: zoom.clamp(MIN_ZOOM, MAX_ZOOM),
            pan,
        }
    }

    fn zoom(&self) -> f32 {
        self.zoom
    }

    fn pan(&self) -> Point<Pixels> {
        self.pan
    }

    /// 缩放到 `zoom`，`anchor` 屏幕点下的内容保持不动。
    fn zoom_to(&mut self, zoom: f32, anchor: Point<Pixels>) {
        let k0 = self.zoom;
        let k1 = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        if k1 == k0 {
            return;
        }
        let local = point((anchor.x - self.pan.x) / k0, (anchor.y - self.pan.y) / k0);
        self.pan = point(anchor.x - local.x * k1, anchor.y - local.y * k1);
        self.zoom = k1;
    }

    fn set_zoom(&mut self, zoom: f32) {
        self.zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
    }

    fn pan_to(&mut self, pan: Point<Pixels>) {
        self.pan = pan;
    }
}

struct HelloWorld {
    camera: Camera,
    drag_start: Option<(Point<Pixels>, Point<Pixels>)>,
    cards: Entity<CardsCanvas>,
    hud_x: Entity<InputState>,
    hud_y: Entity<InputState>,
    hud_zoom: Entity<InputState>,
    _subscriptions: [Subscription; 3],
}

fn sync_hud_input(input: &Entity<InputState>, value: String, window: &mut Window, cx: &mut App) {
    input.update(cx, |state, cx| {
        if !state.focus_handle(cx).is_focused(window) && state.text().to_string() != value {
            state.set_value(value, window, cx);
        }
    });
}

impl HelloWorld {
    fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        cards: Entity<CardsCanvas>,
        initial_pan: Point<Pixels>,
    ) -> Self {
        let hud_x = cx.new(|cx| {
            InputState::new(window, cx).default_value(format!("{:.1}", initial_pan.x.as_f32()))
        });
        let hud_y = cx.new(|cx| {
            InputState::new(window, cx).default_value(format!("{:.1}", initial_pan.y.as_f32()))
        });
        let hud_zoom = cx.new(|cx| InputState::new(window, cx).default_value("1.000"));

        let sub_x =
            cx.subscribe_in(
                &hud_x,
                window,
                |this, _, event: &InputEvent, window, cx| match event {
                    InputEvent::PressEnter { .. } => {
                        this.commit_hud_x(window, cx);
                        window.blur(cx);
                    }
                    _ => {}
                },
            );

        let sub_y =
            cx.subscribe_in(
                &hud_y,
                window,
                |this, _, event: &InputEvent, window, cx| match event {
                    InputEvent::PressEnter { .. } => {
                        this.commit_hud_y(window, cx);
                        window.blur(cx);
                    }
                    _ => {}
                },
            );

        let sub_zoom = cx.subscribe_in(
            &hud_zoom,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } => {
                    this.commit_hud_zoom(window, cx);
                    window.blur(cx);
                }
                _ => {}
            },
        );

        Self {
            camera: Camera::new(1.0, initial_pan),
            drag_start: None,
            cards,
            hud_x,
            hud_y,
            hud_zoom,
            _subscriptions: [sub_x, sub_y, sub_zoom],
        }
    }

    fn commit_hud_x(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.hud_x.read(cx).text().to_string();
        if let Ok(val) = text.trim().parse::<f32>() {
            if val.is_finite() {
                self.camera.pan_to(point(px(val), self.camera.pan().y));
            }
        }
        let norm_val = format!("{:.1}", self.camera.pan().x.as_f32());
        self.hud_x.update(cx, |state, cx| {
            if state.text().to_string() != norm_val {
                state.set_value(norm_val, window, cx);
            }
        });
        cx.notify();
    }

    fn commit_hud_y(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.hud_y.read(cx).text().to_string();
        if let Ok(val) = text.trim().parse::<f32>() {
            if val.is_finite() {
                self.camera.pan_to(point(self.camera.pan().x, px(val)));
            }
        }
        let norm_val = format!("{:.1}", self.camera.pan().y.as_f32());
        self.hud_y.update(cx, |state, cx| {
            if state.text().to_string() != norm_val {
                state.set_value(norm_val, window, cx);
            }
        });
        cx.notify();
    }

    fn commit_hud_zoom(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.hud_zoom.read(cx).text().to_string();
        if let Ok(val) = text.trim().parse::<f32>() {
            if val.is_finite() {
                let anchor = window.viewport_size().center();
                self.camera.zoom_to(val, anchor);
            }
        }
        let norm_val = format!("{:.3}", self.camera.zoom());
        self.hud_zoom.update(cx, |state, cx| {
            if state.text().to_string() != norm_val {
                state.set_value(norm_val, window, cx);
            }
        });
        cx.notify();
    }

    fn camera_hud(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let field_bg = hsla(0., 0., 1., 0.08);
        let hud_fg = hsla(0., 0., 0.98, 1.);
        let hud_muted = hsla(0., 0., 0.62, 1.);
        let field = move |label: &'static str, input: &Entity<InputState>| {
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(div().w(px(40.)).text_color(hud_muted).child(label))
                .child(
                    Input::new(input)
                        .small()
                        .appearance(false)
                        .h(px(22.))
                        .w(px(88.))
                        .px_1()
                        .rounded(px(3.))
                        .bg(field_bg)
                        .text_size(px(10.))
                        .text_color(hud_fg),
                )
        };
        div()
            .absolute()
            .top(px(12.))
            .left(px(12.))
            .occlude()
            .flex()
            .flex_col()
            .w(px(168.))
            .gap_1()
            .px_2()
            .py_1p5()
            .rounded(px(4.))
            .bg(hsla(0., 0., 0.04, 0.92))
            .font_family("Consolas")
            .text_size(px(10.))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _, _, cx| cx.stop_propagation()),
            )
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(|_, _, _, cx| cx.stop_propagation()),
            )
            .on_scroll(cx.listener(|_, _: &ScrollEvent, _, cx| cx.stop_propagation()))
            .child(field("X", &self.hud_x))
            .child(field("Y", &self.hud_y))
            .child(field("ZOOM", &self.hud_zoom))
    }
}

impl Render for HelloWorld {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let zoom = self.camera.zoom();
        let pan = self.camera.pan();
        let is_dragging = self.drag_start.is_some();

        sync_hud_input(&self.hud_x, format!("{:.1}", pan.x.as_f32()), window, cx);
        sync_hud_input(&self.hud_y, format!("{:.1}", pan.y.as_f32()), window, cx);
        sync_hud_input(&self.hud_zoom, format!("{:.3}", zoom), window, cx);

        let ref_size: Size<Pixels> = Size {
            width: px(4016.0),
            height: px(5986.0),
        };
        let mut card_style = StyleRefinement::default();
        card_style.size.width = Some(ref_size.width.into());
        card_style.size.height = Some(ref_size.height.into());
        let cached_cards = self.cards.clone().cached(card_style);

        div()
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(green(500).opacity(0.5))
            .when(is_dragging, |this| this.cursor(CursorStyle::Crosshair))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _, window, cx| {
                    window.blur(cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    window.blur(cx);
                    this.drag_start = Some((event.position, this.camera.pan()));
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
                        let new_pan = start_pan + (event.position - start_pos);
                        if this.camera.pan() != new_pan {
                            this.camera.pan_to(new_pan);
                            cx.notify();
                        }
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
                            l.y / 10.0 * 24.0 // 10x faster if hold ctrl
                        }
                    } // 24px line height
                };
                let factor = (1.0 + delta_y * 0.002).clamp(0.8, 1.25); // 每 1px 垂直增量 ≈ 0.2% 的乘性缩放（0.002）
                let k1 = this.camera.zoom() * factor;
                this.camera.zoom_to(k1, event.position);
                cx.notify();
            }))
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .child(ScaledPaintZoom::new(zoom, pan, ref_size, cached_cards)),
            )
            .child(
                div()
                    .absolute()
                    .top_0()
                    .right_0()
                    .child(fps_monitor(window, cx)),
            )
            .child(self.camera_hud(cx))
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

                let cards = cx.new(|_| CardsCanvas::new(notes));

                let initial_pan = point(
                    px((bounds.size.width / 2.).as_f32()),
                    px((bounds.size.height / 2.).as_f32()),
                );
                cx.new(|cx| HelloWorld::new(window, cx, cards, initial_pan))
            },
        )
        .expect("Failed to open window");
    });
}
