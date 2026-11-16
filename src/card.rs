use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::text::TextView;
use gpui_kit::component::{blue, red, white};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{StatefulInteractiveElement as _, *};

pub const MD_SAMPLE: &str = r#"# Markdown 示例

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

pub const CARD_WIDTH: f32 = 380.;

/// 最小卡片尺寸 (宽, 高)，世界像素。
pub const MIN_CARD_SIZE: (Pixels, Pixels) = (px(120.), px(80.));

pub struct CanvasItem {
    pub world_pos: Point<Pixels>,
    pub view: Entity<CardView>,
}

pub struct CardView {
    button_red: bool,
    editing: bool,
    markdown: SharedString,
    textarea_state: Option<Entity<TextareaState>>,
    /// `layout_as_root` 得到的 border-box。有值后才走 `Entity::cached`。
    pub resolved: Option<Size<Pixels>>,
    /// 卡片根节点样式；悬停时在 `on_hover` 里改 `border_color` 并 `notify` 重绘。
    card_style: StyleRefinement,
    /// 用户拖拽后的固定 border-box 尺寸；`None` 时宽度取 `CARD_WIDTH`、高度由内容决定。
    fixed_size: Option<Size<Pixels>>,
}

impl CardView {
    pub fn new() -> Self {
        let mut card_style = StyleRefinement::default();
        card_style.border_color = Some(red(500));
        Self {
            button_red: true,
            editing: false,
            markdown: MD_SAMPLE.into(),
            textarea_state: None,
            resolved: None,
            card_style,
            fixed_size: None,
        }
    }

    /// 在展示态与编辑态之间切换；切回展示态时同步文本内容，切入编辑态时按需创建 `TextareaState`。
    fn toggle_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing {
            if let Some(state) = &self.textarea_state {
                self.markdown = state.read(cx).value();
            }
            self.editing = false;
        } else {
            let md = self.markdown.clone();
            self.textarea_state.get_or_insert_with(|| {
                cx.new(|cx| {
                    TextareaState::new(window, cx)
                        .default_value(md)
                        .auto_grow(1, 100)
                })
            });
            self.editing = true;
        }
        cx.notify();
    }

    /// 写入新的固定尺寸。尺寸已知，直接更新 `resolved`，无需再测量；只重绘这一张卡片。
    pub fn resize(&mut self, size: Size<Pixels>, cx: &mut Context<Self>) {
        self.fixed_size = Some(size);
        self.resolved = Some(size);
        cx.notify();
    }

    fn set_border_hovered(style: &mut StyleRefinement, hovered: bool) {
        style.border_color = Some(if hovered { red(300) } else { red(500) });
    }

    /// 用上一帧 Taffy 尺寸生成确定的缓存样式。
    pub fn cached_style(size: Size<Pixels>) -> StyleRefinement {
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
            .w(self.fixed_size.map_or(px(CARD_WIDTH), |s| s.width))
            .when_some(self.fixed_size, |this, s| {
                this.h(s.height).overflow_hidden()
            })
            .flex()
            .flex_col()
            .flex_auto()
            .gap_3()
            .p_4()
            .rounded_md()
            .bg(button_color.opacity(0.5))
            .border_5()
            .border_color(card_border)
            // .hover is useless because refreshing can still hit cache
            .on_hover(cx.listener(|this, hovered, _, cx| {
                Self::set_border_hovered(&mut this.card_style, *hovered);
                cx.notify();
            }))
            .child(if self.editing {
                let state = self
                    .textarea_state
                    .as_ref()
                    .expect("textarea_state initialized when editing");
                div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .child(Textarea::new(state).w_full())
                    .into_any_element()
            } else {
                // textview的文本选择状态更新会将所有的卡片标记dirty，导致大量重复全量刷新
                TextView::markdown(("md-canvas-demo", cx.entity_id()), self.markdown.clone())
                    .selectable(false)
                    .into_any_element()
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("color-toggle-0")
                            .flex_1()
                            .custom(button_style)
                            .label("切换颜色")
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.button_red = !this.button_red;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("mode-toggle-0")
                            .flex_1()
                            .custom(button_style)
                            .label(if self.editing { "展示" } else { "编辑" })
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.toggle_edit(window, cx);
                            })),
                    ),
            )
    }
}
