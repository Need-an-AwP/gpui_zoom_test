use crate::canvas::Camera;
use gpui_kit::component::searchable_list::SearchableVec;
use gpui_kit::component::select::{Select, SelectItem, SelectState};
use gpui_kit::component::{IndexPath, Sizable as _};
use gpui_kit::*;

/// 卡片选择器的选项；value 为卡片在 items 中的下标。
#[derive(Clone)]
pub struct CardOption {
    pub index: usize,
    pub title: SharedString,
}

impl SelectItem for CardOption {
    type Value = usize;

    fn title(&self) -> SharedString {
        self.title.clone()
    }

    fn value(&self) -> &usize {
        &self.index
    }
}

pub type CardSelectState = SelectState<SearchableVec<CardOption>>;

/// 构建卡片搜索跳转选择器状态。初始不选中任何卡片。
pub fn build_card_select(
    item_count: usize,
    grid_cols: usize,
    window: &mut Window,
    cx: &mut App,
) -> Entity<CardSelectState> {
    let options: Vec<CardOption> = (0..item_count)
        .map(|index| CardOption {
            index,
            title: format!(
                "Card {} (r{}, c{})",
                index + 1,
                index / grid_cols,
                index % grid_cols
            )
            .into(),
        })
        .collect();
    cx.new(|cx| {
        SelectState::new(SearchableVec::new(options), None::<IndexPath>, window, cx).searchable(true)
    })
}

/// 仅文本显示的相机 HUD，不拦截任何输入事件之外的开销。
pub fn camera_hud(camera: &Camera, card_select: &Entity<CardSelectState>) -> impl IntoElement {
    let hud_fg = hsla(0., 0., 0.98, 1.);
    let hud_muted = hsla(0., 0., 0.62, 1.);
    let pan = camera.pan();
    let row = move |label: &'static str, value: String| {
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .child(div().w(px(40.)).text_color(hud_muted).child(label))
            .child(div().text_color(hud_fg).child(value))
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
        .child(row("X", format!("{:.1}", pan.x.as_f32())))
        .child(row("Y", format!("{:.1}", pan.y.as_f32())))
        .child(row("ZOOM", format!("{:.3}", camera.zoom())))
        .child(
            Select::new(card_select)
                .id("card-select")
                .xsmall()
                .placeholder("跳转到卡片")
                .w_full(),
        )
}
