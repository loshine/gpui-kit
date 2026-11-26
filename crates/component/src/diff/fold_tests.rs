//! Nested folds of unchanged ranges, as in IntelliJ IDEA's diff viewer.

use std::ops::Range;

use gpui::{AppContext as _, Entity, TestAppContext};

use super::{
    DiffFile, DiffLinePosition, DiffMode, DiffSide, DiffState,
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

    fn hover(cx: &mut VisualTestContext, selector: &'static str) {
        let bounds = cx.debug_bounds(selector).expect(selector);
        cx.simulate_mouse_move(bounds.center(), None, Default::default());
        draw(cx);
    }

    #[gpui::test]
    fn the_anchor_on_the_line_numbers_folds_its_level(cx: &mut TestAppContext) {
        let (state, cx) = open(cx, DiffMode::Split);
        click(cx, "fold-new-5".into(), false);
        assert_eq!(last_line(&state, cx), Some(9));
        // Hidden until the pointer is over the line numbers, and only on
        // the first line of the level.
        assert!(cx.debug_bounds("fold-anchor-5").is_none());
        hover(cx, "gutter-new-8");
        let anchor = cx.debug_bounds("fold-anchor-5").expect("anchor");
        let line = cx.debug_bounds("gutter-new-6").expect("line 6");
        assert_eq!(anchor.center().y, line.center().y);
        assert!(anchor.right() <= line.left());
        // Over the code it hides again.
        hover(cx, "fold-new-9");
        assert!(cx.debug_bounds("fold-anchor-5").is_none());
        // The left column shows its own.
        hover(cx, "gutter-old-7");
        let anchor = cx.debug_bounds("fold-anchor-5").expect("left anchor");
        assert!(anchor.right() <= cx.debug_bounds("gutter-old-6").unwrap().left());
        cx.simulate_click(anchor.center(), Default::default());
        draw(cx);
        assert_eq!(last_line(&state, cx), Some(5));
        assert!(cx.debug_bounds("fold-anchor-5").is_none());
    }

    #[gpui::test]
    fn unified_shows_the_anchor_after_both_line_numbers(cx: &mut TestAppContext) {
        let (state, cx) = open(cx, DiffMode::Unified);
        click(cx, "fold-5".into(), false);
        click(cx, "fold-9".into(), false);
        assert_eq!(last_line(&state, cx), Some(17));
        hover(cx, "gutter-old-3");
        let anchor = cx.debug_bounds("fold-anchor-5").expect("anchor");
        assert!(anchor.left() >= cx.debug_bounds("gutter-old-6").unwrap().right());
        assert!(cx.debug_bounds("fold-anchor-9").is_some());
        cx.simulate_click(anchor.center(), Default::default());
        draw(cx);
        assert_eq!(last_line(&state, cx), Some(5));
        // The deeper level kept its state.
        click(cx, "fold-5".into(), false);
        assert_eq!(last_line(&state, cx), Some(17));
    }

    struct Rooted {
        state: Entity<DiffState>,
    }

    impl Render for Rooted {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .w(px(600.))
                .h(px(900.))
                .child(Diff::new(&self.state).size_full())
        }
    }

    fn open_rooted(
        cx: &mut TestAppContext,
        mode: DiffMode,
        lines: usize,
    ) -> (Entity<DiffState>, VisualTestContext) {
        cx.update(|cx| {
            crate::init(cx);
            cx.set_reduce_motion(true);
        });
        let state = cx.new(|cx| {
            DiffState::new([document("long.txt", lines, &[1])], cx)
                .with_mode(mode)
                .with_context_lines(Some(4))
                .with_nested_folds(true)
        });
        let window = cx.update(|cx| {
            let rooted = cx.new(|_| Rooted {
                state: state.clone(),
            });
            cx.open_window(
                gpui::WindowOptions {
                    window_bounds: Some(gpui::WindowBounds::Windowed(gpui::Bounds::new(
                        gpui::point(px(0.), px(0.)),
                        gpui::size(px(800.), px(1000.)),
                    ))),
                    ..Default::default()
                },
                |window, cx| cx.new(|cx| crate::Root::new(rooted, window, cx)),
            )
            .unwrap()
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        draw(&mut cx);
        (state, cx)
    }

    fn wait(cx: &mut VisualTestContext) {
        cx.executor()
            .advance_clock(std::time::Duration::from_secs(1));
        draw(cx);
    }

    #[gpui::test]
    fn resting_on_a_fold_previews_the_hidden_lines(cx: &mut TestAppContext) {
        let (_, mut cx) = open_rooted(cx, DiffMode::Split, 20);
        let cx = &mut cx;
        assert!(cx.debug_bounds("fold-preview-line-6").is_none());
        hover(cx, "fold-new-5");
        wait(cx);
        // Lines 6 to the empty line 21, as IntelliJ IDEA lists them.
        for line in [6, 13, 21] {
            assert!(
                cx.debug_bounds(format!("fold-preview-line-{line}").leak())
                    .is_some(),
                "line {line}"
            );
        }
        assert!(cx.debug_bounds("fold-preview-line-5").is_none());
    }

    #[gpui::test]
    fn the_preview_lists_only_what_half_the_window_holds(cx: &mut TestAppContext) {
        let (_, mut cx) = open_rooted(cx, DiffMode::Unified, 200);
        let cx = &mut cx;
        hover(cx, "fold-5");
        wait(cx);
        let first = cx.debug_bounds("fold-preview-line-6").expect("first line");
        assert!(cx.debug_bounds("fold-preview-line-120").is_none());
        let shown = (6..120)
            .filter(|line| {
                cx.debug_bounds(format!("fold-preview-line-{line}").leak())
                    .is_some()
            })
            .count();
        // Half the window, and the row cut off at its edge.
        let half = cx.update(|window, _| f32::from(window.viewport_size().height) / 2.);
        let height = f32::from(first.size.height);
        // Row bounds snap to whole pixels.
        let rows = shown as f32;
        assert!(
            rows * height >= half && (rows - 1.) * (height - 1.) < half,
            "{shown} rows of {height} in {half}"
        );
        assert!(shown > 10, "{shown} rows");
    }
}

#[gpui::test]
fn folding_a_level_again_keeps_deeper_levels(cx: &mut TestAppContext) {
    let state = state(cx, document("long.txt", 20, &[1]), DiffMode::Split);
    click_first_fold(&state, cx);
    click_first_fold(&state, cx);
    assert_eq!(last_shown(&state, cx), Some(17));
    state.update(cx, |state, cx| state.collapse_fold(0, 5..21, cx));
    assert_eq!(last_shown(&state, cx), Some(5));
    // The level below was left expanded and shows again with this one.
    click_first_fold(&state, cx);
    assert_eq!(last_shown(&state, cx), Some(17));
    // After expanding everything, one level folds on its own.
    state.update(cx, |state, cx| state.expand_unchanged(cx));
    state.update(cx, |state, cx| state.collapse_fold(0, 9..21, cx));
    assert_eq!(last_shown(&state, cx), Some(9));
    click_first_fold(&state, cx);
    assert_eq!(last_shown(&state, cx), Some(21));
}

#[gpui::test]
fn collapsing_at_a_line_folds_its_innermost_expanded_level(cx: &mut TestAppContext) {
    let state = state(cx, document("long.txt", 20, &[1]), DiffMode::Split);
    let at = |line| DiffLinePosition::new("long.txt", DiffSide::Modified, line);
    state.update(cx, |state, cx| state.expand_unchanged(cx));
    // Line 3 lies in no level.
    assert!(!state.update(cx, |state, cx| state.collapse_fold_at(&at(3), cx)));
    assert_eq!(last_shown(&state, cx), Some(21));
    // Line 18 lies in all three; the innermost folds.
    assert!(state.update(cx, |state, cx| state.collapse_fold_at(&at(18), cx)));
    assert_eq!(last_shown(&state, cx), Some(17));
    assert!(state.update(cx, |state, cx| state.collapse_fold_at(&at(12), cx)));
    assert_eq!(last_shown(&state, cx), Some(9));
    assert!(state.update(cx, |state, cx| state.collapse_fold_at(&at(7), cx)));
    assert_eq!(last_shown(&state, cx), Some(5));
    // A folded line is in no expanded level.
    assert!(!state.update(cx, |state, cx| state.collapse_fold_at(&at(7), cx)));
}

#[gpui::test]
fn expanded_levels_carry_over_to_a_new_reading(cx: &mut TestAppContext) {
    let state = state(cx, document("long.txt", 20, &[1]), DiffMode::Split);
    click_first_fold(&state, cx);
    assert_eq!(last_shown(&state, cx), Some(9));
    let memory = state
        .read_with(cx, |state, _| state.fold_memory("long.txt"))
        .unwrap();
    // Line 2 changes too: the levels move down one line.
    state.update(cx, |state, cx| {
        state.set_files([document("long.txt", 20, &[1, 2])], cx);
    });
    assert_eq!(last_shown(&state, cx), Some(6));
    state.update(cx, |state, cx| state.restore_folds("long.txt", &memory, cx));
    assert_eq!(last_shown(&state, cx), Some(10));
    // Nothing expanded remembers nothing expanded.
    let folded = state.update(cx, |state, cx| {
        state.collapse_unchanged(cx);
        state.fold_memory("long.txt").unwrap()
    });
    state.update(cx, |state, cx| {
        state.expand_unchanged(cx);
        state.restore_folds("long.txt", &folded, cx);
    });
    assert_eq!(last_shown(&state, cx), Some(6));
    // An unknown path changes nothing.
    assert!(
        state
            .read_with(cx, |state, _| state.fold_memory("other.txt"))
            .is_none()
    );
}

#[gpui::test]
fn switching_the_layout_folds_every_level(cx: &mut TestAppContext) {
    let state = state(cx, document("long.txt", 20, &[1]), DiffMode::Split);
    click_first_fold(&state, cx);
    state.update(cx, |state, cx| state.set_mode(DiffMode::Unified, cx));
    assert_eq!(last_shown(&state, cx), Some(5));
}

#[gpui::test]
fn without_nested_folds_nothing_is_remembered(cx: &mut TestAppContext) {
    let state = cx
        .new(|cx| DiffState::new([document("long.txt", 20, &[1])], cx).with_context_lines(Some(4)));
    assert!(
        state
            .read_with(cx, |state, _| state.fold_memory("long.txt"))
            .is_none()
    );
    // Without nested folds a range stays expanded across layouts, as before.
    click_first_fold(&state, cx);
    state.update(cx, |state, cx| state.set_mode(DiffMode::Unified, cx));
    assert_eq!(last_shown(&state, cx), Some(21));
}
