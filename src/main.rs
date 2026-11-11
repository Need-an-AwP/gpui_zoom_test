mod canvas;

use canvas::{CanvasElement, CanvasFrame, DEFAULT_MAX_ZOOM, DEFAULT_MIN_ZOOM};
use gpui_fps::fps_monitor;
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::text::TextView;
use gpui_kit::component::{black, blue, green, red, white};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    StatefulInteractiveElement as _, *,
};

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

const GRID_COLS: usize = 10;
const GRID_ROWS: usize = 10;
const CARD_WIDTH: f32 = 380.;
const GRID_STEP_X: f32 = CARD_WIDTH + 10.;
/// 初始行距。卡片高度由布局决定，不写进卡片样式。
const GRID_STEP_Y: f32 = 590.;

struct CanvasItem {
    world_pos: Point<Pixels>,
    view: Entity<CardView>,
}

struct CardView {
    button_red: bool,
    /// `layout_as_root` 得到的 border-box。有值后才走 `Entity::cached`。
    resolved: Option<Size<Pixels>>,
    /// 卡片根节点样式；悬停时在 `on_hover` 里改 `border_color` 并 `notify` 重绘。
    card_style: StyleRefinement,
}

impl CardView {
    fn new() -> Self {
        let mut card_style = StyleRefinement::default();
        card_style.border_color = Some(red(500));
        Self {
            button_red: true,
            resolved: None,
            card_style,
        }
    }

    fn set_border_hovered(style: &mut StyleRefinement, hovered: bool) {
        style.border_color = Some(if hovered { red(300) } else { red(500) });
    }

    /// 用上一帧 Taffy 尺寸生成确定的缓存样式。
    fn cached_style(size: Size<Pixels>) -> StyleRefinement {
        let mut style = StyleRefinement::default();
        style.size.width = Some(size.width.into());
        style.size.height = Some(size.height.into());
        style
    }
}

impl Render for CardView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let button_color = if self.button_red { red(500) } else { blue(500) };
        let button_style = ButtonCustomVariant::new(cx)
            .color(button_color)
            .foreground(white())
            .hover(button_color)
            .active(button_color);
        let card_border = self.card_style.border_color.unwrap_or(red(500));

        div()
            .id(("card", cx.entity_id()))
            .w(px(CARD_WIDTH))
            .flex()
            .flex_col()
            .flex_auto()
            .gap_3()
            .p_4()
            .rounded_md()
            .bg(button_color.opacity(0.5))
            .border_5()
            .border_color(card_border)
            // .hover is useless because we skipped refreshing 
            .on_hover(cx.listener(|this, hovered, _, cx| {
                Self::set_border_hovered(&mut this.card_style, *hovered);
                cx.notify();
            }))
            .child(
                TextView::markdown("md-canvas-demo", MD_SAMPLE).selectable(false), // textview的文本选择状态更新会将所有的卡片标记dirty，导致大量重复全量刷新
            )
            .child(
                Button::new("color-toggle-0")
                    .custom(button_style)
                    .label("切换颜色")
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.button_red = !this.button_red;
                        cx.notify();
                    })),
            )
    }
}

struct Camera {
    zoom: f32,
    pan: Point<Pixels>,
}

impl Camera {
    fn new(zoom: f32, pan: Point<Pixels>) -> Self {
        Self {
            zoom: zoom.clamp(DEFAULT_MIN_ZOOM, DEFAULT_MAX_ZOOM),
            pan,
        }
    }

    fn zoom(&self) -> f32 {
        self.zoom
    }

    fn pan(&self) -> Point<Pixels> {
        self.pan
    }

    fn zoom_to(&mut self, zoom: f32, anchor: Point<Pixels>) {
        let k0 = self.zoom;
        let k1 = zoom.clamp(DEFAULT_MIN_ZOOM, DEFAULT_MAX_ZOOM);
        if k1 == k0 {
            return;
        }
        let local = point((anchor.x - self.pan.x) / k0, (anchor.y - self.pan.y) / k0);
        self.pan = point(anchor.x - local.x * k1, anchor.y - local.y * k1);
        self.zoom = k1;
    }

    fn pan_to(&mut self, pan: Point<Pixels>) {
        self.pan = pan;
    }
}

struct HelloWorld {
    camera: Camera,
    drag_start: Option<(Point<Pixels>, Point<Pixels>)>,
    items: Vec<CanvasItem>,
}

impl HelloWorld {
    fn new(items: Vec<CanvasItem>) -> Self {
        Self {
            camera: Camera::new(1.0, Point::default()),
            drag_start: None,
            items,
        }
    }
}

impl Render for HelloWorld {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let zoom = self.camera.zoom();
        let pan = self.camera.pan();

        div()
            .flex()
            .flex_col()
            .size_full()
            .items_center()
            .justify_center()
            .gap_2()
            .child(
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .bg(linear_gradient(
                        135.0,
                        linear_color_stop(black(), 0.2),
                        linear_color_stop(white(), 0.8),
                    ))
                    // .when(is_dragging, |this| this.cursor(CursorStyle::Crosshair))// not working on windows
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
                        let delta_y = match event.delta {
                            ScrollDelta::Pixels(p) => p.y.as_f32(),
                            ScrollDelta::Lines(l) => {
                                if event.modifiers.control {
                                    l.y * 24.0
                                } else {
                                    l.y / 10.0 * 24.0
                                }
                            }
                        };
                        let factor = (1.0 + delta_y * 0.002).clamp(0.8, 1.25);
                        let k1 = this.camera.zoom() * factor;
                        this.camera.zoom_to(k1, event.position);
                        cx.notify();
                    }))
                    .child(
                        CanvasFrame::new(zoom, pan).children(self.items.iter().map(|item| {
                            let resolved = item.view.read(cx).resolved;
                            let child = match resolved {
                                Some(size) => item
                                    .view
                                    .clone()
                                    .cached(CardView::cached_style(size))
                                    .into_any_element(),
                                None => item.view.clone().into_any_element(),
                            };
                            let view = item.view.clone();
                            CanvasElement::new(item.world_pos, child).on_measured(
                                move |measured, cx| {
                                    // 只收下第一次内容测量。之后空缓存节点的回声尺寸不再回写。
                                    if view.read(cx).resolved.is_some() {
                                        return false;
                                    }
                                    view.update(cx, |card, _cx| {
                                        card.resolved = Some(measured);
                                    });
                                    true
                                },
                            )
                        })),
                    ),
            )
            .child(div().absolute().top_0().right_0().child(
                fps_monitor(window, cx).frame_budget(std::time::Duration::from_nanos(8_333_333)),
            ))
    }
}

fn main() {
    application().with_assets(assets::Assets).run(|cx| {
        init(cx);

        let bounds = Bounds::centered(None, size(px(1280.), px(720.)), cx);
        open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("canvas frame demo".into()),
                    ..TitlebarOptions::default()
                }),
                ..Default::default()
            },
            cx,
            |_, cx| {
                let origin = point(px(40.), px(40.));
                let mut items = Vec::with_capacity(GRID_COLS * GRID_ROWS);
                for row in 0..GRID_ROWS {
                    for col in 0..GRID_COLS {
                        let world_pos = point(
                            origin.x + px(col as f32 * GRID_STEP_X),
                            origin.y + px(row as f32 * GRID_STEP_Y),
                        );
                        items.push(CanvasItem {
                            world_pos,
                            view: cx.new(|_| CardView::new()),
                        });
                    }
                }
                cx.new(|_| HelloWorld::new(items))
            },
        )
        .expect("Failed to open window");
    });
}
