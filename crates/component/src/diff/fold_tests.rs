//! Nested folds of unchanged ranges, as in IntelliJ IDEA's diff viewer.

use std::ops::Range;

use gpui::{AppContext as _, Entity, TestAppContext};

use super::{
    DiffFile, DiffMode, DiffSide, DiffState,
    state::{DisplayRow, FoldExpansion, fold_levels},
};

/// A file of `lines` lines ending with a line break, with the 1-based
/// `changed` lines edited, as a patch carrying the whole file as context.
fn document(path: &str, lines: usize, changed: &[usize]) -> DiffFile {
    let mut body = String::new();
    for line in 1..=lines {
        if changed.contains(&line) {
            body.push_str(&format!("-line {line}\n+edited {line}\n"));
        } else {
            body.push_str(&format!(" line {line}\n"));
        }
    }
    let patch = format!(
        "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -1,{lines} +1,{lines} @@\n{body}"
    );
    DiffFile::parse(&patch)
        .unwrap()
        .remove(0)
        .with_trailing_lines([DiffSide::Original, DiffSide::Modified])
}

fn state(cx: &mut TestAppContext, document: DiffFile, mode: DiffMode) -> Entity<DiffState> {
    cx.new(|cx| {
        let mut state = DiffState::new([document], cx)
            .with_context_lines(Some(4))
            .with_nested_folds(true);
        state.set_mode(mode, cx);
        state
    })
}

/// The modified line numbers shown as code, and `None` for each fold row.
fn shown(state: &DiffState) -> Vec<Option<usize>> {
    state
        .rows()
        .iter()
        .filter_map(|row| match row {
            DisplayRow::Code { modified, .. } => modified.map(|ix| Some(ix + 1)),
            DisplayRow::Fold { .. } => Some(None),
            _ => None,
        })
        .collect()
}

fn folds(state: &DiffState) -> Vec<Range<usize>> {
    state
        .rows()
        .iter()
        .filter_map(|row| match row {
            DisplayRow::Fold { pairs, .. } => Some(pairs.clone()),
            _ => None,
        })
        .collect()
}

fn click_first_fold(state: &Entity<DiffState>, cx: &mut TestAppContext) {
    state.update(cx, |state, cx| {
        let pairs = folds(state).remove(0);
        state.expand_fold(0, pairs, FoldExpansion::All, cx);
    });
}

fn last_shown(state: &Entity<DiffState>, cx: &mut TestAppContext) -> Option<usize> {
    state.read_with(cx, |state, _| shown(state).into_iter().flatten().last())
}

#[test]
fn levels_double_the_context_and_reach_the_hunk_edges() {
    // `long.txt`: 21 lines with the trailing one, line 1 changed.
    assert_eq!(fold_levels(1..21, 0..21, 4, 2), [5..21, 9..21, 17..21]);
    // `mid.txt`: 61 lines, line 30 changed.
    assert_eq!(fold_levels(0..29, 0..61, 4, 2), [0..25, 0..21, 0..13]);
    assert_eq!(fold_levels(30..61, 0..61, 4, 2), [34..61, 38..61, 46..61]);
    // `two.txt`: 41 lines, lines 5 and 35 changed.
    assert!(fold_levels(0..4, 0..41, 4, 2).is_empty());
    assert_eq!(fold_levels(5..34, 0..41, 4, 2), [9..30, 13..26]);
    assert_eq!(fold_levels(35..41, 0..41, 4, 2), [39..41]);
}

#[test]
fn levels_hiding_too_few_lines_do_not_exist() {
    // One line past the context.
    assert!(fold_levels(1..6, 0..6, 4, 2).is_empty());
    assert_eq!(fold_levels(1..7, 0..7, 4, 2), [5..7]);
    // Between two changes the inner levels run out first.
    assert_eq!(fold_levels(5..15, 0..30, 4, 2), [9..11]);
    assert!(fold_levels(5..14, 0..30, 4, 2).is_empty());
    // A whole unchanged hunk is one level.
    assert_eq!(fold_levels(0..10, 0..10, 4, 2), [0..10]);
    assert_eq!(fold_levels(0..10, 0..10, 0, 2), [0..10]);
}

#[gpui::test]
fn each_click_reveals_one_level(cx: &mut TestAppContext) {
    for mode in [DiffMode::Split, DiffMode::Unified] {
        let state = state(cx, document("long.txt", 20, &[1]), mode);
        state.read_with(cx, |state, _| {
            assert_eq!(
                shown(state),
                [Some(1), Some(2), Some(3), Some(4), Some(5), None],
                "{mode:?}"
            );
        });
        for last in [9, 17] {
            click_first_fold(&state, cx);
            assert_eq!(last_shown(&state, cx), Some(last), "{mode:?}");
            state.read_with(cx, |state, _| assert_eq!(folds(state).len(), 1));
        }
        click_first_fold(&state, cx);
        assert_eq!(last_shown(&state, cx), Some(21), "{mode:?}");
        state.read_with(cx, |state, _| assert!(folds(state).is_empty()));
    }
}

#[gpui::test]
fn ranges_between_and_after_changes_fold_separately(cx: &mut TestAppContext) {
    let state = state(cx, document("two.txt", 40, &[5, 35]), DiffMode::Split);
    state.read_with(cx, |state, _| {
        assert_eq!(folds(state), [9..30, 39..41]);
        let shown = shown(state);
        assert_eq!(
            &shown[..10],
            (1..=9).map(Some).chain([None]).collect::<Vec<_>>()
        );
        assert_eq!(shown.last(), Some(&None));
    });
    click_first_fold(&state, cx);
    state.read_with(cx, |state, _| {
        assert_eq!(folds(state), [13..26, 39..41]);
        let shown = shown(state);
        let expected = (1..=13)
            .map(Some)
            .chain([None])
            .chain((27..=39).map(Some))
            .chain([None])
            .collect::<Vec<_>>();
        assert_eq!(shown, expected);
    });
}

#[gpui::test]
fn ranges_before_a_change_fold_to_the_start(cx: &mut TestAppContext) {
    let state = state(cx, document("mid.txt", 60, &[30]), DiffMode::Split);
    state.read_with(cx, |state, _| assert_eq!(folds(state), [0..25, 34..61]));
    click_first_fold(&state, cx);
    state.read_with(cx, |state, _| assert_eq!(folds(state), [0..21, 34..61]));
    click_first_fold(&state, cx);
    state.read_with(cx, |state, _| assert_eq!(folds(state), [0..13, 34..61]));
}

#[gpui::test]
fn turning_folds_off_and_on_folds_every_level(cx: &mut TestAppContext) {
    let state = state(cx, document("long.txt", 20, &[1]), DiffMode::Split);
    click_first_fold(&state, cx);
    state.update(cx, |state, cx| state.set_context_lines(None, cx));
    assert_eq!(last_shown(&state, cx), Some(21));
    state.read_with(cx, |state, _| assert!(folds(state).is_empty()));
    state.update(cx, |state, cx| state.set_context_lines(Some(4), cx));
    assert_eq!(last_shown(&state, cx), Some(5));
}

#[gpui::test]
fn expanding_everything_reveals_every_level(cx: &mut TestAppContext) {
    let state = state(cx, document("long.txt", 20, &[1]), DiffMode::Split);
    state.update(cx, |state, cx| state.expand_unchanged(cx));
    assert_eq!(last_shown(&state, cx), Some(21));
    state.update(cx, |state, cx| state.collapse_unchanged(cx));
    assert_eq!(last_shown(&state, cx), Some(5));
}

#[gpui::test]
fn without_nested_folds_a_range_folds_once(cx: &mut TestAppContext) {
    let state = cx
        .new(|cx| DiffState::new([document("long.txt", 20, &[1])], cx).with_context_lines(Some(4)));
    state.read_with(cx, |state, _| assert_eq!(folds(state), [5..21]));
    click_first_fold(&state, cx);
    assert_eq!(last_shown(&state, cx), Some(21));
}

mod rendered {
    use gpui::{
        AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
        TestAppContext, VisualTestContext, Window, div, px,
    };

    use super::{document, last_shown};
    use crate::diff::{Diff, DiffGutterMarker, DiffLinePosition, DiffMode, DiffSide, DiffState};

    struct Host {
        state: Entity<DiffState>,
    }

    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            // A divider column, as applications with change markers show it.
            let diff = Diff::new(&self.state)
                .size_full()
                .divider_markers([DiffGutterMarker::new(
                    "change",
                    DiffLinePosition::new("long.txt", DiffSide::Modified, 1),
                )])
                .render_divider_marker(|_, _, _| div().size_3());
            div().w(px(600.)).h(px(900.)).child(diff)
        }
    }

    fn draw(cx: &mut VisualTestContext) {
        for _ in 0..3 {
            cx.run_until_parked();
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
        }
    }

    fn open(
        cx: &mut TestAppContext,
        mode: DiffMode,
    ) -> (Entity<DiffState>, &mut VisualTestContext) {
        cx.update(crate::init);
        let state = cx.new(|cx| {
            DiffState::new([document("long.txt", 20, &[1])], cx)
                .with_mode(mode)
                .with_context_lines(Some(4))
                .with_nested_folds(true)
        });
        let (_, cx) = cx.add_window_view({
            let state = state.clone();
            move |_, _| Host { state }
        });
        draw(cx);
        (state, cx)
    }

    fn click(cx: &mut VisualTestContext, selector: String, left: bool) {
        let bounds = cx
            .debug_bounds(selector.clone().leak())
            .expect("fold column");
        let x = if left {
            bounds.left() + px(4.)
        } else {
            bounds.center().x
        };
        cx.simulate_click(gpui::point(x, bounds.center().y), Default::default());
        draw(cx);
    }

    fn last_line(state: &Entity<DiffState>, cx: &mut VisualTestContext) -> Option<usize> {
        last_shown(state, cx)
    }

    #[gpui::test]
    fn clicking_either_column_reveals_one_level_in_both(cx: &mut TestAppContext) {
        let (state, cx) = open(cx, DiffMode::Split);
        assert_eq!(last_line(&state, cx), Some(5));
        // Where the line numbers are, then in the code, then in the left column.
        click(cx, "fold-new-5".into(), true);
        assert_eq!(last_line(&state, cx), Some(9));
        click(cx, "fold-new-9".into(), false);
        assert_eq!(last_line(&state, cx), Some(17));
        click(cx, "fold-old-17".into(), false);
        assert_eq!(last_line(&state, cx), Some(21));
        assert!(cx.debug_bounds("fold-new-17").is_none());
    }

    #[gpui::test]
    fn the_divider_draws_the_line_but_takes_no_click(cx: &mut TestAppContext) {
        let (state, cx) = open(cx, DiffMode::Split);
        let old = cx.debug_bounds("fold-old-5").expect("left column");
        let divider = cx.debug_bounds("fold-divider-5").expect("divider");
        let new = cx.debug_bounds("fold-new-5").expect("right column");
        assert_eq!(divider.left(), old.right());
        assert_eq!(new.left(), divider.right());
        click(cx, "fold-divider-5".into(), false);
        assert_eq!(last_line(&state, cx), Some(5));
    }

    #[gpui::test]
    fn hovering_a_column_highlights_the_whole_row(cx: &mut TestAppContext) {
        let (state, cx) = open(cx, DiffMode::Split);
        let new = cx.debug_bounds("fold-new-5").expect("right column");
        cx.simulate_mouse_move(new.center(), None, Default::default());
        draw(cx);
        assert_eq!(
            cx.update(|_, cx| state.read(cx).hovered_fold()),
            Some((0, 5))
        );
        let divider = cx.debug_bounds("fold-divider-5").expect("divider");
        cx.simulate_mouse_move(divider.center(), None, Default::default());
        draw(cx);
        assert_eq!(cx.update(|_, cx| state.read(cx).hovered_fold()), None);
    }

    #[gpui::test]
    fn unified_folds_span_one_column(cx: &mut TestAppContext) {
        let (state, cx) = open(cx, DiffMode::Unified);
        assert!(cx.debug_bounds("fold-new-5").is_none());
        for last in [9, 17, 21] {
            let start = state.read_with(cx, |state, _| super::folds(state)[0].start);
            click(cx, format!("fold-{start}"), false);
            assert_eq!(last_line(&state, cx), Some(last));
        }
    }
}
