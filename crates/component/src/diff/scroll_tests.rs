use gpui::{
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, ScrollDelta,
    ScrollEvent, Styled as _, TestAppContext, VisualTestContext, Window, div, point, px,
};

use super::{Diff, DiffScrollAnchor, DiffSide, DiffState, document::fixture, state::DisplayRow};

/// Every line changed, so the unified patch is two rows per line.
const LINES: usize = 300;

struct Host {
    state: Entity<DiffState>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(400.))
            .h(px(200.))
            .child(Diff::new(&self.state).size_full())
    }
}

fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        _ = window.draw(cx);
    });
}

fn long_patch(cx: &mut TestAppContext) -> (Entity<DiffState>, &mut VisualTestContext) {
    cx.update(crate::init);
    let original = (0..LINES)
        .map(|ix| format!("old {ix}\n"))
        .collect::<String>();
    let modified = (0..LINES)
        .map(|ix| format!("new {ix}\n"))
        .collect::<String>();
    let state =
        cx.new(|cx| DiffState::new([fixture::modified("long.txt", &original, &modified)], cx));
    let (_, cx) = cx.add_window_view({
        let state = state.clone();
        move |_, _| Host { state }
    });
    // The body records its size on the first paint and lays out again.
    for _ in 0..3 {
        draw(cx);
    }
    (state, cx)
}

#[gpui::test]
fn the_whole_patch_keeps_its_height_after_the_first_layout(cx: &mut TestAppContext) {
    let (state, cx) = long_patch(cx);
    let (max, viewport) = cx.update(|_, cx| {
        let list = state.read(cx).list();
        (
            list.max_offset_for_scrollbar().y,
            list.viewport_bounds().size.height,
        )
    });
    // Rows past the viewport have not been rendered; without their hinted
    // height the patch would read as barely taller than one screen.
    assert!(
        max > viewport * 10.,
        "max offset {max:?} for a {viewport:?} viewport"
    );
}

#[gpui::test]
fn the_wheel_scrolls_past_the_rows_rendered_so_far(cx: &mut TestAppContext) {
    let (state, cx) = long_patch(cx);
    for _ in 0..50 {
        cx.simulate_event(ScrollEvent {
            position: point(px(100.), px(120.)),
            delta: ScrollDelta::Pixels(point(px(0.), px(-40.))),
            ..Default::default()
        });
        draw(cx);
    }
    let top = cx.update(|_, cx| state.read(cx).list().logical_scroll_top().item_ix);
    // 2000 px of wheel travel crosses far more than the first screen of rows.
    assert!(top > 40, "scrolled to row {top}");
}

#[gpui::test]
fn the_wheel_reports_scrolled_but_step_change_does_not(cx: &mut TestAppContext) {
    let (state, cx) = long_patch(cx);
    let scrolled = std::rc::Rc::new(std::cell::Cell::new(0));
    let _subscription = cx.update(|_, cx| {
        let scrolled = scrolled.clone();
        cx.subscribe(&state, move |_, event: &super::DiffEvent, _| {
            if matches!(event, super::DiffEvent::Scrolled) {
                scrolled.set(scrolled.get() + 1);
            }
        })
    });
    state.update(cx, |state, cx| {
        state.step_change(true, None, cx);
    });
    draw(cx);
    assert_eq!(scrolled.get(), 0);
    let before = top(&state, cx);
    wheel(cx, 3, -40.);
    assert_ne!(top(&state, cx), before);
    assert!(scrolled.get() > 0);
}

fn open<'a>(
    cx: &'a mut TestAppContext,
    original: &str,
    modified: &str,
) -> (Entity<DiffState>, &'a mut VisualTestContext) {
    cx.update(crate::init);
    let state =
        cx.new(|cx| DiffState::new([fixture::modified("file.txt", original, modified)], cx));
    let (_, cx) = cx.add_window_view({
        let state = state.clone();
        move |_, _| Host { state }
    });
    for _ in 0..3 {
        draw(cx);
    }
    (state, cx)
}

fn wheel(cx: &mut VisualTestContext, times: usize, delta: f32) {
    for _ in 0..times {
        cx.simulate_event(ScrollEvent {
            position: point(px(100.), px(120.)),
            delta: ScrollDelta::Pixels(point(px(0.), px(delta))),
            ..Default::default()
        });
        draw(cx);
    }
}

fn top(state: &Entity<DiffState>, cx: &mut VisualTestContext) -> gpui::ListOffset {
    cx.update(|_, cx| state.read(cx).list().logical_scroll_top())
}

/// Reinstalls the same file, which resets the viewport, then restores `anchor`.
fn reinstall(
    state: &Entity<DiffState>,
    cx: &mut VisualTestContext,
    anchor: &DiffScrollAnchor,
    side: DiffSide,
) {
    state.update(cx, |state, cx| {
        let files = state.files().to_vec();
        state.set_files(files, cx);
        state.scroll_to_anchor(anchor, side, cx);
    });
    draw(cx);
}

#[gpui::test]
fn an_anchor_restores_the_top_row_and_offset_after_set_files(cx: &mut TestAppContext) {
    let (state, cx) = long_patch(cx);
    wheel(cx, 20, -37.);
    let before = top(&state, cx);
    assert!(
        before.item_ix > 0 && before.offset_in_item > px(0.),
        "{before:?}"
    );
    let anchor = cx.update(|_, cx| state.read(cx).scroll_anchor()).unwrap();
    assert_eq!(anchor.rows_above, 0);
    for side in [DiffSide::Original, DiffSide::Modified] {
        reinstall(&state, cx, &anchor, side);
        assert_eq!(top(&state, cx), before, "{side:?}");
    }
}

#[gpui::test]
fn an_anchor_at_the_file_header_keeps_the_header_in_view(cx: &mut TestAppContext) {
    let original = (1..=40)
        .map(|ix| format!("line {ix}\n"))
        .collect::<String>();
    let modified = original.replace("line 2\n", "line 2 changed\n");
    let (state, cx) = open(cx, &original, &modified);
    let anchor = cx.update(|_, cx| state.read(cx).scroll_anchor()).unwrap();
    assert_eq!((anchor.original, anchor.modified), (Some(1), Some(1)));
    assert!(anchor.rows_above > 0, "{anchor:?}");
    reinstall(&state, cx, &anchor, DiffSide::Modified);
    assert_eq!(top(&state, cx).item_ix, 0);
}

#[gpui::test]
fn an_anchor_inside_a_collapsed_range_stops_at_the_fold_row(cx: &mut TestAppContext) {
    let original = (1..=300)
        .map(|ix| format!("line {ix}\n"))
        .collect::<String>();
    let modified = original
        .replace("line 1\n", "line 1 changed\n")
        .replace("line 150\n", "line 150 changed\n")
        .replace("line 300\n", "line 300 changed\n");
    let (state, cx) = open(cx, &original, &modified);
    // Inside the range collapsed between the first two changes.
    let anchor = DiffScrollAnchor {
        path: "file.txt".into(),
        original: Some(75),
        modified: Some(75),
        rows_above: 0,
        offset: px(0.),
    };
    let rows_before = cx.update(|_, cx| state.read(cx).list().item_count());
    state.update(cx, |state, cx| {
        state.scroll_to_anchor(&anchor, DiffSide::Original, cx)
    });
    draw(cx);
    let (rows_after, row) = cx.update(|_, cx| {
        let state = state.read(cx);
        let ix = state.list().logical_scroll_top().item_ix;
        (state.list().item_count(), state.rows().get(ix).cloned())
    });
    // Nothing expanded; the top row is the fold itself.
    assert_eq!(rows_after, rows_before);
    assert!(matches!(row, Some(DisplayRow::Fold { .. })), "{row:?}");
}
