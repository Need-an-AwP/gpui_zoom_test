mod app;
mod canvas;
mod card;
mod hud;

use app::CanvasWorkspace;
use card::{CARD_WIDTH, CanvasItem, CardView};
use gpui_kit::*;

const GRID_COLS: usize = 10;
const GRID_ROWS: usize = 10;
const GRID_STEP_X: f32 = CARD_WIDTH + 10.;
/// 初始行距。卡片高度由布局决定，不写进卡片样式。
const GRID_STEP_Y: f32 = 590.;

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
            |window, cx| {
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
                cx.new(|cx| CanvasWorkspace::new(items, GRID_COLS, window, cx))
            },
        )
        .expect("Failed to open window");
    });
}
