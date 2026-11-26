use gpui::{
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
    TestAppContext, VisualTestContext, Window, div, px,
};

use super::{
    Diff, DiffFile, DiffLineRange, DiffMode, DiffSide, DiffState, document::LinePair,
    document::fixture, has_line_ending_change, state::DisplayRow,
};

const BOTH: [DiffSide; 2] = [DiffSide::Original, DiffSide::Modified];

fn line_numbers(document: &DiffFile, side: DiffSide) -> Vec<usize> {
    document
        .lines(side)
        .iter()
        .map(|line| line.line_number())
        .collect()
}

fn last_pair(document: &DiffFile) -> LinePair {
    *document.pairs().last().expect("a pair")
}

fn changed_pairs(document: &DiffFile) -> Vec<(Option<usize>, Option<usize>)> {
    document
        .pairs()
        .iter()
        .filter(|pair| pair.is_changed())
        .map(|pair| (pair.original(), pair.modified()))
        .collect()
}

/// A Git patch with the whole file as context, so the hunk reaches the end.
fn full_patch(path: &str, original: &str, modified: &str) -> DiffFile {
    let body = patch_body(original, modified);
    let old_count = original.split_inclusive('\n').count();
    let new_count = modified.split_inclusive('\n').count();
    let patch = format!(
        "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -1,{old_count} +1,{new_count} @@\n{body}"
    );
    DiffFile::parse(&patch).unwrap().remove(0)
}

fn patch_body(original: &str, modified: &str) -> String {
    use similar::{ChangeTag, TextDiff};
    let old_lines: Vec<&str> = original.split_inclusive('\n').collect();
    let new_lines: Vec<&str> = modified.split_inclusive('\n').collect();
    let diff = TextDiff::from_slices(&old_lines, &new_lines);
    let mut body = String::new();
    for op in diff.ops() {
        for change in diff.iter_changes(op) {
            body.push(match change.tag() {
                ChangeTag::Equal => ' ',
                ChangeTag::Delete => '-',
                ChangeTag::Insert => '+',
            });
            body.push_str(change.value());
            if !change.value().ends_with('\n') {
                body.push_str("\n\\ No newline at end of file\n");
            }
        }
    }
    body
}

#[test]
fn sides_ending_with_a_line_break_gain_one_unchanged_line() {
    let plain = fixture::modified("a.txt", "a\nb\n", "a\nc\n");
    let document = plain.clone().with_trailing_lines(BOTH);
    for side in BOTH {
        assert_eq!(line_numbers(&document, side), [1, 2, 3]);
        let line = document.lines(side).last().unwrap();
        assert_eq!(line.text().as_ref(), "");
        assert_eq!(line.source(), 4..4);
        assert_eq!(line.counterpart(), Some(2));
    }
    assert_eq!(last_pair(&document), LinePair::new(Some(2), Some(2), false));
    assert_eq!(document.pairs().len(), plain.pairs().len() + 1);
    assert_eq!(changed_pairs(&document), changed_pairs(&plain));
    assert_eq!(
        (document.additions(), document.deletions()),
        (plain.additions(), plain.deletions())
    );
    assert!(document.patch_ix(DiffSide::Modified, 2).is_none());
    assert_eq!(document.patch_lines().len(), plain.patch_lines().len());
    // Without the request nothing changes.
    assert_eq!(line_numbers(&plain, DiffSide::Modified), [1, 2]);
}

#[test]
fn sides_without_a_final_line_break_gain_nothing() {
    let plain = fixture::modified("a.txt", "a\nb", "a\nc");
    let document = plain.clone().with_trailing_lines(BOTH);
    for side in BOTH {
        assert_eq!(line_numbers(&document, side), [1, 2]);
    }
    assert_eq!(document.pairs(), plain.pairs());
}

#[test]
fn one_side_files_mark_their_trailing_line() {
    let added = fixture::document(None, Some(("new.txt", "a\nb\nc\n")))
        .with_trailing_lines([DiffSide::Modified]);
    assert_eq!(line_numbers(&added, DiffSide::Modified), [1, 2, 3, 4]);
    assert_eq!(added.lines_count(DiffSide::Original), 0);
    assert_eq!(last_pair(&added), LinePair::new(None, Some(3), true));
    assert_eq!((added.additions(), added.deletions()), (3, 0));

    let deleted = fixture::document(Some(("old.txt", "a\nb\nc\n")), None)
        .with_trailing_lines([DiffSide::Original]);
    assert_eq!(line_numbers(&deleted, DiffSide::Original), [1, 2, 3, 4]);
    assert_eq!(last_pair(&deleted), LinePair::new(Some(3), None, true));
    assert_eq!((deleted.additions(), deleted.deletions()), (0, 3));

    let unterminated = fixture::document(None, Some(("new.txt", "a\nb\nc")))
        .with_trailing_lines([DiffSide::Modified]);
    assert_eq!(line_numbers(&unterminated, DiffSide::Modified), [1, 2, 3]);
}

#[test]
fn empty_sides_are_one_empty_line() {
    for (document, side) in [
        (
            fixture::document(None, Some(("empty.txt", ""))),
            DiffSide::Modified,
        ),
        (
            fixture::document(Some(("empty.txt", "")), None),
            DiffSide::Original,
        ),
    ] {
        let document = document.with_trailing_lines([side]);
        assert_eq!(line_numbers(&document, side), [1]);
        assert_eq!(line_numbers(&document, side.other()), Vec::<usize>::new());
        assert_eq!(document.pairs().len(), 1);
        assert!(document.pairs()[0].is_changed());
        assert_eq!((document.additions(), document.deletions()), (0, 0));
    }

    // An added file shown beside its empty index entry (intent to add): the
    // empty document's only line sits beside the added side's trailing line.
    let intent = fixture::document(None, Some(("new.txt", "a\nb\n"))).with_trailing_lines(BOTH);
    assert_eq!(line_numbers(&intent, DiffSide::Original), [1]);
    assert_eq!(line_numbers(&intent, DiffSide::Modified), [1, 2, 3]);
    assert_eq!(last_pair(&intent), LinePair::new(Some(0), Some(2), false));
}

#[test]
fn whole_file_references_gain_an_unchanged_line_on_their_only_side() {
    let document = DiffFile::unchanged("a.txt", "a\nb\n").with_trailing_lines(BOTH);
    assert_eq!(line_numbers(&document, DiffSide::Modified), [1, 2, 3]);
    assert_eq!(document.lines_count(DiffSide::Original), 0);
    assert_eq!(last_pair(&document), LinePair::new(None, Some(2), false));
    assert!(!document.has_changes());

    let empty = DiffFile::unchanged("empty.txt", "").with_trailing_lines(BOTH);
    assert_eq!(line_numbers(&empty, DiffSide::Modified), [1]);
    assert_eq!(empty.pairs(), [LinePair::new(None, Some(0), false)]);
}

#[test]
fn trailing_lines_join_the_last_hunk_and_widen_line_numbers() {
    let original = (1..=9).map(|n| format!("line {n}\n")).collect::<String>();
    let modified = original.replacen("line 1\n", "LINE 1\n", 1);
    let document = full_patch("nine.txt", &original, &modified).with_trailing_lines(BOTH);
    let hunk = document.hunks().last().unwrap();
    assert_eq!(hunk.pairs().end, document.pairs().len());
    assert_eq!(hunk.lines(DiffSide::Original).end, 10);
    assert_eq!(hunk.lines(DiffSide::Modified).end, 10);
    assert_eq!(document.line_number_digits(), 2);
}

#[test]
fn only_the_final_line_break_differs() {
    // (original, modified, changed pairs as (original, modified) line indexes)
    for (original, modified, changed) in [
        // A final line break added: only the new trailing line is added.
        ("x\ny", "x\ny\n", vec![(None, Some(2))]),
        // Removed: only the old trailing line is removed.
        ("line 3\n", "line 3", vec![(Some(1), None)]),
        // Lines appended after an unterminated last line.
        ("x\ny", "x\ny\nz", vec![(None, Some(2))]),
        // CRLF against no break at all.
        ("same\r\n", "same", vec![(Some(1), None)]),
    ] {
        let document = fixture::modified("a.txt", original, modified).with_trailing_lines(BOTH);
        assert_eq!(
            changed_pairs(&document),
            changed,
            "{original:?} -> {modified:?}"
        );
        // Change counts stay as Git reports them.
        let plain = fixture::modified("a.txt", original, modified);
        assert_eq!(
            (document.additions(), document.deletions()),
            (plain.additions(), plain.deletions())
        );
    }

    // A line ending changed in the middle of the file stays a change.
    let document = fixture::modified("a.txt", "a\r\nb\n", "a\nb\n").with_trailing_lines(BOTH);
    assert_eq!(changed_pairs(&document), [(Some(0), Some(0))]);

    // A different last line is still a change.
    let document = fixture::modified("a.txt", "x\ny", "x\nz\n").with_trailing_lines(BOTH);
    assert_eq!(
        changed_pairs(&document),
        [(Some(1), Some(1)), (None, Some(2))]
    );

    // Without the request the pair keeps its line-ending change.
    let plain = fixture::modified("a.txt", "same\n", "same");
    assert_eq!(changed_pairs(&plain), [(Some(0), Some(0))]);
    assert!(has_line_ending_change(
        &plain,
        DiffSide::Original,
        0,
        Some(0)
    ));
}

#[test]
fn copying_the_trailing_line_adds_nothing() {
    let document =
        fixture::document(None, Some(("new.txt", "a\nb\nc\n"))).with_trailing_lines(BOTH);
    let side = DiffSide::Modified;
    assert_eq!(document.text_for_lines(side, 1, 4), "a\nb\nc\n");
    assert_eq!(document.text_for_lines(side, 3, 4), "c\n");
    assert_eq!(document.text_for_lines(side, 4, 4), "");
}

struct Host {
    state: Entity<DiffState>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(600.))
            .h(px(900.))
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

fn open(
    cx: &mut TestAppContext,
    file: DiffFile,
    mode: DiffMode,
    context: Option<usize>,
    one_side: bool,
) -> (Entity<DiffState>, &mut VisualTestContext) {
    cx.update(crate::init);
    let state = cx.new(|cx| {
        DiffState::new([file], cx)
            .with_mode(mode)
            .with_context_lines(context)
            .with_one_side_files(one_side)
    });
    let (_, cx) = cx.add_window_view({
        let state = state.clone();
        move |_, _| Host { state }
    });
    draw(cx);
    (state, cx)
}

fn has(cx: &mut VisualTestContext, selector: &'static str) -> bool {
    cx.debug_bounds(selector).is_some()
}

#[gpui::test]
fn unified_shows_both_numbers_on_the_trailing_row(cx: &mut TestAppContext) {
    let file = full_patch("a.txt", "a\nb\nc\n", "a\nx\nb\nc\n").with_trailing_lines(BOTH);
    let (_, cx) = open(cx, file, DiffMode::Unified, None, false);
    let old = cx.debug_bounds("gutter-old-4").expect("old trailing line");
    let new = cx.debug_bounds("gutter-new-5").expect("new trailing line");
    assert_eq!(old.origin.y, new.origin.y);
    assert!(!has(cx, "gutter-old-5"));
    assert!(!has(cx, "gutter-new-6"));
}

#[gpui::test]
fn trailing_lines_fold_with_other_unchanged_lines(cx: &mut TestAppContext) {
    let original = (1..=20).map(|n| format!("line {n}\n")).collect::<String>();
    let modified = original.replacen("line 1\n", "LINE 1\n", 1);
    let file = full_patch("long.txt", &original, &modified).with_trailing_lines(BOTH);
    let pairs = file.pairs().len();
    assert_eq!(pairs, 21);
    let (state, cx) = open(cx, file, DiffMode::Split, Some(3), false);
    let fold = state.read_with(cx, |state, _| {
        state.rows().iter().find_map(|row| match row {
            DisplayRow::Fold { pairs, .. } => Some(pairs.clone()),
            _ => None,
        })
    });
    // Line 1 changed: lines 2-4 stay as context, lines 5-21 fold, including
    // the trailing line 21.
    assert_eq!(fold, Some(4..pairs));
    assert!(!has(cx, "gutter-new-21"));
    state.update(cx, |state, cx| state.expand_unchanged(cx));
    draw(cx);
    assert!(has(cx, "gutter-new-21"));
}

#[gpui::test]
fn the_trailing_line_can_be_selected(cx: &mut TestAppContext) {
    let file = fixture::document(None, Some(("new.txt", "a\nb\nc\n")))
        .with_trailing_lines([DiffSide::Modified]);
    for mode in [DiffMode::Split, DiffMode::Unified] {
        let (state, cx) = open(cx, file.clone(), mode, None, true);
        assert!(has(cx, "gutter-new-4"), "{mode:?}");
        for (start, end, text) in [(4, 4, ""), (3, 4, "c\n")] {
            state.update(cx, |state, cx| {
                state.set_selected_lines(
                    Some(DiffLineRange::new(
                        "new.txt",
                        DiffSide::Modified,
                        start,
                        end,
                    )),
                    cx,
                )
            });
            draw(cx);
            let (selected, copied) = state.read_with(cx, |state, cx| {
                (state.selected_lines(), state.selected_text(cx))
            });
            assert_eq!(
                selected.map(|range| (range.start(), range.end())),
                Some((start, end)),
                "{mode:?}"
            );
            assert_eq!(copied, text, "{mode:?} {start}-{end}");
        }
    }
}

#[gpui::test]
fn empty_one_side_files_show_a_selectable_line(cx: &mut TestAppContext) {
    for (patch, side, gutter, notice) in [
        (
            "diff --git a/empty.txt b/empty.txt\nnew file mode 100644\nindex 0000000..e69de29\n",
            DiffSide::Modified,
            "gutter-new-1",
            "empty-line-new",
        ),
        (
            "diff --git a/empty.txt b/empty.txt\ndeleted file mode 100644\nindex e69de29..0000000\n",
            DiffSide::Original,
            "gutter-old-1",
            "empty-line-old",
        ),
    ] {
        for mode in [DiffMode::Split, DiffMode::Unified] {
            let file = DiffFile::parse(patch)
                .unwrap()
                .remove(0)
                .with_trailing_lines([side]);
            let (state, cx) = open(cx, file, mode, None, true);
            assert!(has(cx, gutter), "{gutter} {mode:?}");
            assert!(!has(cx, notice), "{notice} {mode:?}");
            state.update(cx, |state, cx| {
                state.set_selected_lines(Some(DiffLineRange::new("empty.txt", side, 1, 1)), cx)
            });
            assert!(
                state.read_with(cx, |state, _| state.selected_lines().is_some()),
                "{gutter} {mode:?}"
            );
        }
    }
}

#[gpui::test]
fn unified_selection_across_sides_ends_at_the_trailing_line(cx: &mut TestAppContext) {
    let file = full_patch("a.txt", "a\nb\nc\n", "a\nx\nc\n").with_trailing_lines(BOTH);
    let (state, cx) = open(cx, file, DiffMode::Unified, None, false);
    state.update(cx, |state, cx| {
        state.set_selected_lines(
            Some(
                DiffLineRange::new("a.txt", DiffSide::Original, 2, 4)
                    .with_end_side(DiffSide::Modified),
            ),
            cx,
        )
    });
    draw(cx);
    let copied = state.read_with(cx, |state, cx| state.selected_text(cx));
    assert_eq!(copied, "b\nx\nc\n");
}
