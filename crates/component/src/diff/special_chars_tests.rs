use gpui::{
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
    TestAppContext, VisualTestContext, Window, div, px,
};

use super::{
    Diff, DiffFile, DiffMode, DiffSide, DiffState,
    document::fixture,
    presentation::{DiffInlineUnit, changed_runs},
};

fn nul_file() -> DiffFile {
    fixture::modified(
        "nul.txt",
        "line one\nline\0two\n",
        "line one\nline\0two changed\n",
    )
    .with_special_characters()
}

/// The source text between two display offsets of one line, as a drag
/// selection over the painted text copies it.
fn dragged(file: &DiffFile, side: DiffSide, ix: usize, from: usize, to: usize) -> String {
    let line = &file.lines(side)[ix];
    let start = line.source().start + line.source_offset(from);
    let end = line.source().start + line.source_offset(to);
    file.source(side)[start..end].to_owned()
}

#[test]
fn dragging_over_a_box_copies_the_original_character() {
    let file = nul_file();
    let line = &file.lines(DiffSide::Modified)[1];
    assert_eq!(line.display().as_str(), "lineNULtwo changed");
    // From `e` to the end of `t`: the box is one character.
    assert_eq!(dragged(&file, DiffSide::Modified, 1, 3, 8), "e\0t");
    // From the right half of the box to the end of `two`.
    assert_eq!(dragged(&file, DiffSide::Modified, 1, 6, 10), "two");
    // From its left half, the character is included.
    assert_eq!(dragged(&file, DiffSide::Modified, 1, 5, 10), "\0two");
}

#[test]
fn inline_changes_leave_an_unchanged_box_unhighlighted() {
    let file =
        fixture::modified("ctl.txt", "esc a\u{1b}b\n", "esc a\u{1b}c\n").with_special_characters();
    let (old, new) = changed_runs(
        file.lines(DiffSide::Original)[0].text(),
        file.lines(DiffSide::Modified)[0].text(),
        DiffInlineUnit::Word,
    );
    for (side, runs) in [(DiffSide::Original, old), (DiffSide::Modified, new)] {
        let line = &file.lines(side)[0];
        assert_eq!(&line.display()[5..8], "ESC");
        let painted: Vec<_> = runs
            .iter()
            .map(|run| line.display_range(run.start as usize..run.end as usize))
            .collect();
        assert_eq!(painted, [8..9], "{side:?}");
    }
}

struct Host {
    state: Entity<DiffState>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(600.))
            .h(px(400.))
            .child(Diff::new(&self.state).size_full())
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

#[gpui::test]
fn boxes_paint_in_both_modes_and_with_a_selection(cx: &mut TestAppContext) {
    cx.update(crate::init);
    for mode in [DiffMode::Split, DiffMode::Unified] {
        let state = cx.new(|cx| DiffState::new([nul_file()], cx).with_mode(mode));
        let (_, cx) = cx.add_window_view({
            let state = state.clone();
            move |_, _| Host { state }
        });
        draw(cx);
        assert!(cx.debug_bounds("source-new-1").is_some(), "{mode:?}");
        state.update(cx, |state, cx| {
            state.set_selected_lines(
                Some(super::DiffLineRange::new(
                    "nul.txt",
                    DiffSide::Modified,
                    2,
                    2,
                )),
                cx,
            );
            assert_eq!(state.selected_text(cx), "line\0two changed\n");
        });
        draw(cx);
    }
}
