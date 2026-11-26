//! Invisible and control characters shown as boxed Unicode abbreviations.
//!
//! The table and the two context rules follow IntelliJ IDEA 2026.3
//! (`SpecialCharacterFragment.SPECIAL_CHAR_CODES` and
//! `SpecialCharacterFragment.create`), so text that IDEA marks reads the
//! same in a diff: control codes, bidirectional controls, and invisible
//! spaces and joiners. Tab, line feed and carriage return are not marked.

use unicode_properties::{GeneralCategory, UnicodeGeneralCategory as _};

/// The abbreviation drawn in place of `ch`, or `None` when the character
/// paints as ordinary text.
pub(crate) fn abbreviation(ch: char) -> Option<&'static str> {
    Some(match ch {
        '\u{00}' => "NUL",
        '\u{01}' => "SOH",
        '\u{02}' => "STX",
        '\u{03}' => "ETX",
        '\u{04}' => "EOT",
        '\u{05}' => "ENQ",
        '\u{06}' => "ACK",
        '\u{07}' => "BEL",
        '\u{08}' => "BS",
        '\u{0B}' => "VT",
        '\u{0C}' => "FF",
        '\u{0E}' => "SO",
        '\u{0F}' => "SI",
        '\u{10}' => "DLE",
        '\u{11}' => "DC1",
        '\u{12}' => "DC2",
        '\u{13}' => "DC3",
        '\u{14}' => "DC4",
        '\u{15}' => "NAK",
        '\u{16}' => "SYN",
        '\u{17}' => "ETB",
        '\u{18}' => "CAN",
        '\u{19}' => "EM",
        '\u{1A}' => "SUB",
        '\u{1B}' => "ESC",
        '\u{1C}' => "FS",
        '\u{1D}' => "GS",
        '\u{1E}' => "RS",
        '\u{1F}' => "US",
        '\u{7F}' => "DEL",
        // U+0080 has no entry; it paints as ordinary text.
        '\u{81}' => "HOP",
        '\u{82}' => "BPH",
        '\u{83}' => "NBH",
        '\u{84}' => "IND",
        '\u{85}' => "NEL",
        '\u{86}' => "SSA",
        '\u{87}' => "ESA",
        '\u{88}' => "HTS",
        '\u{89}' => "HTJ",
        '\u{8A}' => "VTS",
        '\u{8B}' => "PLD",
        '\u{8C}' => "PLU",
        '\u{8D}' => "RI",
        '\u{8E}' => "SS2",
        '\u{8F}' => "SS3",
        '\u{90}' => "DCS",
        '\u{91}' => "PU1",
        '\u{92}' => "PU2",
        '\u{93}' => "STS",
        '\u{94}' => "CCH",
        '\u{95}' => "MW",
        '\u{96}' => "SPA",
        '\u{97}' => "EPA",
        '\u{98}' => "SOS",
        '\u{99}' => "SGCI",
        '\u{9A}' => "SCI",
        '\u{9B}' => "CSI",
        '\u{9C}' => "ST",
        '\u{9D}' => "OSC",
        '\u{9E}' => "PM",
        '\u{9F}' => "APC",
        '\u{A0}' => "NBSP",
        '\u{AD}' => "SHY",
        '\u{34F}' => "CGJ",
        '\u{61C}' => "ALM",
        '\u{2000}' => "NQSP",
        '\u{2001}' => "MQSP",
        '\u{2002}' => "ENSP",
        '\u{2003}' => "EMSP",
        '\u{2004}' => "3/MSP",
        '\u{2005}' => "4/MSP",
        '\u{2006}' => "6/MSP",
        '\u{2007}' => "FSP",
        '\u{2008}' => "PSP",
        '\u{2009}' => "THSP",
        '\u{200A}' => "HSP",
        '\u{200B}' => "ZWSP",
        '\u{200C}' => "ZWNJ",
        '\u{200D}' => "ZWJ",
        '\u{200E}' => "LRM",
        '\u{200F}' => "RLM",
        '\u{2028}' => "LSEP",
        '\u{2029}' => "PSEP",
        '\u{202A}' => "LRE",
        '\u{202B}' => "RLE",
        '\u{202C}' => "PDF",
        '\u{202D}' => "LRO",
        '\u{202E}' => "RLO",
        '\u{202F}' => "NNBSP",
        '\u{205F}' => "MMSP",
        '\u{2060}' => "WJ",
        '\u{2061}' => "f()",
        '\u{2062}' => "x",
        '\u{2063}' => ",",
        '\u{2064}' => "+",
        // U+2065 is unassigned and has no entry.
        '\u{2066}' => "LRI",
        '\u{2067}' => "RLI",
        '\u{2068}' => "FSI",
        '\u{2069}' => "PDI",
        '\u{206A}' => "ISS",
        '\u{206B}' => "ASS",
        '\u{206C}' => "IAFS",
        '\u{206D}' => "AAFS",
        '\u{206E}' => "NADS",
        '\u{206F}' => "NODS",
        '\u{FEFF}' => "ZWNBSP",
        _ => return None,
    })
}

/// The abbreviation for `ch` in its line, given the characters around it.
///
/// A space followed by a combining mark is the mark's base, and a joiner
/// next to non-ASCII text shapes it (emoji sequences, Arabic, Indic
/// scripts); those keep their ordinary rendering.
pub(crate) fn abbreviation_in(
    ch: char,
    previous: Option<char>,
    next: Option<char>,
) -> Option<&'static str> {
    let abbreviation = abbreviation(ch)?;
    let hidden = match ch {
        '\u{A0}' | '\u{2000}'..='\u{200A}' | '\u{202F}' | '\u{205F}' => {
            next.is_some_and(|next| next.general_category() == GeneralCategory::NonspacingMark)
        }
        '\u{200C}' | '\u{200D}' => [previous, next].into_iter().flatten().any(|neighbor| {
            neighbor >= '\u{80}' && neighbor.general_category() != GeneralCategory::Format
        }),
        _ => false,
    };
    (!hidden).then_some(abbreviation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_has_idea_entries_only() {
        let marked = (0..=0x10FFFF)
            .filter_map(char::from_u32)
            .filter(|ch| abbreviation(*ch).is_some())
            .count();
        assert_eq!(marked, 106);
        for ch in [
            '\t',
            '\n',
            '\r',
            '\u{80}',
            '\u{2065}',
            '\u{3000}',
            '\u{E0001}',
            'a',
        ] {
            assert_eq!(abbreviation(ch), None, "{:?}", ch);
        }
        assert_eq!(abbreviation('\0'), Some("NUL"));
        assert_eq!(abbreviation('\u{0C}'), Some("FF"));
        assert_eq!(abbreviation('\u{7F}'), Some("DEL"));
        assert_eq!(abbreviation('\u{85}'), Some("NEL"));
        assert_eq!(abbreviation('\u{200B}'), Some("ZWSP"));
        assert_eq!(abbreviation('\u{2061}'), Some("f()"));
        assert_eq!(abbreviation('\u{FEFF}'), Some("ZWNBSP"));
    }

    #[test]
    fn context_keeps_shaping_characters_unmarked() {
        // A space carrying a combining acute accent.
        assert_eq!(abbreviation_in('\u{A0}', Some('a'), Some('\u{301}')), None);
        assert_eq!(
            abbreviation_in('\u{A0}', Some('a'), Some('b')),
            Some("NBSP")
        );
        // A joiner inside an emoji sequence, and between ASCII letters.
        assert_eq!(
            abbreviation_in('\u{200D}', Some('\u{1F469}'), Some('\u{1F4BB}')),
            None
        );
        assert_eq!(
            abbreviation_in('\u{200D}', Some('a'), Some('b')),
            Some("ZWJ")
        );
        assert_eq!(abbreviation_in('\u{200C}', None, Some('b')), Some("ZWNJ"));
        // A format character next to a joiner does not hide it.
        assert_eq!(
            abbreviation_in('\u{200D}', Some('\u{200E}'), Some('b')),
            Some("ZWJ")
        );
        // Other characters ignore their neighbors.
        assert_eq!(
            abbreviation_in('\0', Some('\u{301}'), Some('\u{301}')),
            Some("NUL")
        );
    }
}
