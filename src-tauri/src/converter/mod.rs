//! Platform-independent conversion between two-set Korean and QWERTY keys.
//!
//! The caller always chooses a direction. This module performs no language
//! detection and has no platform, UI, clipboard, hook, or Tauri dependencies.

const HANGUL_BASE: u32 = 0xAC00;
const HANGUL_END: u32 = 0xD7A3;
const MEDIAL_COUNT: usize = 21;
const FINAL_COUNT: usize = 28;

const LEADS: [char; 19] = [
    'ㄱ', 'ㄲ', 'ㄴ', 'ㄷ', 'ㄸ', 'ㄹ', 'ㅁ', 'ㅂ', 'ㅃ', 'ㅅ', 'ㅆ', 'ㅇ', 'ㅈ', 'ㅉ', 'ㅊ', 'ㅋ',
    'ㅌ', 'ㅍ', 'ㅎ',
];
const MEDIALS: [char; MEDIAL_COUNT] = [
    'ㅏ', 'ㅐ', 'ㅑ', 'ㅒ', 'ㅓ', 'ㅔ', 'ㅕ', 'ㅖ', 'ㅗ', 'ㅘ', 'ㅙ', 'ㅚ', 'ㅛ', 'ㅜ', 'ㅝ', 'ㅞ',
    'ㅟ', 'ㅠ', 'ㅡ', 'ㅢ', 'ㅣ',
];
const FINALS: [char; FINAL_COUNT] = [
    '\0', 'ㄱ', 'ㄲ', 'ㄳ', 'ㄴ', 'ㄵ', 'ㄶ', 'ㄷ', 'ㄹ', 'ㄺ', 'ㄻ', 'ㄼ', 'ㄽ', 'ㄾ', 'ㄿ', 'ㅀ',
    'ㅁ', 'ㅂ', 'ㅄ', 'ㅅ', 'ㅆ', 'ㅇ', 'ㅈ', 'ㅊ', 'ㅋ', 'ㅌ', 'ㅍ', 'ㅎ',
];

const LEAD_KEYS: [&str; 19] = [
    "r", "R", "s", "e", "E", "f", "a", "q", "Q", "t", "T", "d", "w", "W", "c", "z", "x", "v", "g",
];
const MEDIAL_KEYS: [&str; MEDIAL_COUNT] = [
    "k", "o", "i", "O", "j", "p", "u", "P", "h", "hk", "ho", "hl", "y", "n", "nj", "np", "nl", "b",
    "m", "ml", "l",
];
const FINAL_KEYS: [&str; FINAL_COUNT] = [
    "", "r", "R", "rt", "s", "sw", "sg", "e", "f", "fr", "fa", "fq", "ft", "fx", "fv", "fg", "a",
    "q", "qt", "t", "T", "d", "w", "c", "z", "x", "v", "g",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversionDirection {
    EnglishToKorean,
    KoreanToEnglish,
}

pub fn convert(text: &str, direction: ConversionDirection) -> String {
    match direction {
        ConversionDirection::EnglishToKorean => convert_to_korean(text),
        ConversionDirection::KoreanToEnglish => convert_to_english(text),
    }
}

pub fn convert_to_korean(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut composer = Composer::default();

    for character in text.chars() {
        match english_key_to_jamo(character) {
            Some(jamo) => composer.push(jamo, &mut output),
            None => {
                composer.flush(&mut output);
                output.push(character);
            }
        }
    }

    composer.flush(&mut output);
    output
}

pub fn convert_to_english(text: &str) -> String {
    let mut output = String::with_capacity(text.len());

    for character in text.chars() {
        if let Some((lead, medial, final_index)) = decompose_syllable(character) {
            output.push_str(LEAD_KEYS[lead]);
            output.push_str(MEDIAL_KEYS[medial]);
            output.push_str(FINAL_KEYS[final_index]);
        } else if let Some(keys) = jamo_to_keys(character) {
            output.push_str(keys);
        } else {
            output.push(character);
        }
    }

    output
}

#[derive(Debug, Clone, Copy)]
enum Jamo {
    Consonant {
        compatibility: char,
        lead: Option<usize>,
        final_index: Option<usize>,
    },
    Vowel(usize),
}

#[derive(Default)]
struct Composer {
    lead: Option<usize>,
    medial: Option<usize>,
    final_index: Option<usize>,
}

impl Composer {
    fn push(&mut self, jamo: Jamo, output: &mut String) {
        match jamo {
            Jamo::Consonant {
                compatibility,
                lead,
                final_index,
            } => self.push_consonant(compatibility, lead, final_index, output),
            Jamo::Vowel(medial) => self.push_vowel(medial, output),
        }
    }

    fn push_consonant(
        &mut self,
        compatibility: char,
        lead: Option<usize>,
        possible_final: Option<usize>,
        output: &mut String,
    ) {
        match (self.lead, self.medial, self.final_index) {
            (None, None, None) => {
                if let Some(lead) = lead {
                    self.lead = Some(lead);
                } else {
                    output.push(compatibility);
                }
            }
            (Some(_), None, None) | (None, Some(_), None) => {
                self.flush(output);
                if let Some(lead) = lead {
                    self.lead = Some(lead);
                } else {
                    output.push(compatibility);
                }
            }
            (Some(_), Some(_), None) => {
                if let Some(final_index) = possible_final {
                    self.final_index = Some(final_index);
                } else {
                    self.flush(output);
                    if let Some(lead) = lead {
                        self.lead = Some(lead);
                    } else {
                        output.push(compatibility);
                    }
                }
            }
            (Some(_), Some(_), Some(current_final)) => {
                if let Some(next_final) =
                    possible_final.and_then(|next| combine_finals(current_final, next))
                {
                    self.final_index = Some(next_final);
                } else {
                    self.flush(output);
                    if let Some(lead) = lead {
                        self.lead = Some(lead);
                    } else {
                        output.push(compatibility);
                    }
                }
            }
            _ => {
                self.flush(output);
                output.push(compatibility);
            }
        }
    }

    fn push_vowel(&mut self, vowel: usize, output: &mut String) {
        match (self.lead, self.medial, self.final_index) {
            (None, None, None) => self.medial = Some(vowel),
            (Some(_), None, None) => self.medial = Some(vowel),
            (None, Some(current), None) => {
                if let Some(combined) = combine_vowels(current, vowel) {
                    self.medial = Some(combined);
                } else {
                    self.flush(output);
                    self.medial = Some(vowel);
                }
            }
            (Some(_), Some(current), None) => {
                if let Some(combined) = combine_vowels(current, vowel) {
                    self.medial = Some(combined);
                } else {
                    self.flush(output);
                    self.medial = Some(vowel);
                }
            }
            (Some(lead), Some(medial), Some(final_index)) => {
                if let Some((remaining_final, next_lead)) = split_final(final_index) {
                    output.push(compose_syllable(lead, medial, Some(remaining_final)));
                    self.lead = Some(next_lead);
                } else if let Some(next_lead) = final_to_lead(final_index) {
                    output.push(compose_syllable(lead, medial, None));
                    self.lead = Some(next_lead);
                } else {
                    output.push(compose_syllable(lead, medial, Some(final_index)));
                    self.lead = None;
                }

                self.medial = Some(vowel);
                self.final_index = None;
            }
            _ => {
                self.flush(output);
                self.medial = Some(vowel);
            }
        }
    }

    fn flush(&mut self, output: &mut String) {
        match (self.lead, self.medial, self.final_index) {
            (Some(lead), Some(medial), final_index) => {
                output.push(compose_syllable(lead, medial, final_index));
            }
            (Some(lead), None, _) => output.push(LEADS[lead]),
            (None, Some(medial), _) => output.push(MEDIALS[medial]),
            _ => {}
        }

        self.lead = None;
        self.medial = None;
        self.final_index = None;
    }
}

fn english_key_to_jamo(key: char) -> Option<Jamo> {
    let (compatibility, lead, final_index) = match key {
        'r' => ('ㄱ', Some(0), Some(1)),
        'R' => ('ㄲ', Some(1), Some(2)),
        's' | 'S' => ('ㄴ', Some(2), Some(4)),
        'e' => ('ㄷ', Some(3), Some(7)),
        'E' => ('ㄸ', Some(4), None),
        'f' | 'F' => ('ㄹ', Some(5), Some(8)),
        'a' | 'A' => ('ㅁ', Some(6), Some(16)),
        'q' => ('ㅂ', Some(7), Some(17)),
        'Q' => ('ㅃ', Some(8), None),
        't' => ('ㅅ', Some(9), Some(19)),
        'T' => ('ㅆ', Some(10), Some(20)),
        'd' | 'D' => ('ㅇ', Some(11), Some(21)),
        'w' => ('ㅈ', Some(12), Some(22)),
        'W' => ('ㅉ', Some(13), None),
        'c' | 'C' => ('ㅊ', Some(14), Some(23)),
        'z' | 'Z' => ('ㅋ', Some(15), Some(24)),
        'x' | 'X' => ('ㅌ', Some(16), Some(25)),
        'v' | 'V' => ('ㅍ', Some(17), Some(26)),
        'g' | 'G' => ('ㅎ', Some(18), Some(27)),
        _ => {
            let vowel = match key {
                'k' | 'K' => 0,
                'o' => 1,
                'O' => 3,
                'i' | 'I' => 2,
                'j' | 'J' => 4,
                'p' => 5,
                'P' => 7,
                'u' | 'U' => 6,
                'h' | 'H' => 8,
                'y' | 'Y' => 12,
                'n' | 'N' => 13,
                'b' | 'B' => 17,
                'm' | 'M' => 18,
                'l' | 'L' => 20,
                _ => return None,
            };
            return Some(Jamo::Vowel(vowel));
        }
    };

    Some(Jamo::Consonant {
        compatibility,
        lead,
        final_index,
    })
}

fn combine_vowels(first: usize, second: usize) -> Option<usize> {
    match (first, second) {
        (8, 0) => Some(9),
        (8, 1) => Some(10),
        (8, 20) => Some(11),
        (13, 4) => Some(14),
        (13, 5) => Some(15),
        (13, 20) => Some(16),
        (18, 20) => Some(19),
        _ => None,
    }
}

fn combine_finals(first: usize, second: usize) -> Option<usize> {
    match (first, second) {
        (1, 19) => Some(3),
        (4, 22) => Some(5),
        (4, 27) => Some(6),
        (8, 1) => Some(9),
        (8, 16) => Some(10),
        (8, 17) => Some(11),
        (8, 19) => Some(12),
        (8, 25) => Some(13),
        (8, 26) => Some(14),
        (8, 27) => Some(15),
        (17, 19) => Some(18),
        _ => None,
    }
}

fn split_final(final_index: usize) -> Option<(usize, usize)> {
    match final_index {
        3 => Some((1, 9)),
        5 => Some((4, 12)),
        6 => Some((4, 18)),
        9 => Some((8, 0)),
        10 => Some((8, 6)),
        11 => Some((8, 7)),
        12 => Some((8, 9)),
        13 => Some((8, 16)),
        14 => Some((8, 17)),
        15 => Some((8, 18)),
        18 => Some((17, 9)),
        _ => None,
    }
}

fn final_to_lead(final_index: usize) -> Option<usize> {
    match final_index {
        1 => Some(0),
        2 => Some(1),
        4 => Some(2),
        7 => Some(3),
        8 => Some(5),
        16 => Some(6),
        17 => Some(7),
        19 => Some(9),
        20 => Some(10),
        21 => Some(11),
        22 => Some(12),
        23 => Some(14),
        24 => Some(15),
        25 => Some(16),
        26 => Some(17),
        27 => Some(18),
        _ => None,
    }
}

fn compose_syllable(lead: usize, medial: usize, final_index: Option<usize>) -> char {
    let offset =
        (lead * MEDIAL_COUNT * FINAL_COUNT) + (medial * FINAL_COUNT) + final_index.unwrap_or(0);
    char::from_u32(HANGUL_BASE + offset as u32).expect("valid modern Hangul syllable")
}

fn decompose_syllable(character: char) -> Option<(usize, usize, usize)> {
    let code = character as u32;
    if !(HANGUL_BASE..=HANGUL_END).contains(&code) {
        return None;
    }

    let offset = (code - HANGUL_BASE) as usize;
    let lead = offset / (MEDIAL_COUNT * FINAL_COUNT);
    let medial = (offset % (MEDIAL_COUNT * FINAL_COUNT)) / FINAL_COUNT;
    let final_index = offset % FINAL_COUNT;
    Some((lead, medial, final_index))
}

fn jamo_to_keys(character: char) -> Option<&'static str> {
    LEADS
        .iter()
        .position(|jamo| *jamo == character)
        .map(|index| LEAD_KEYS[index])
        .or_else(|| {
            MEDIALS
                .iter()
                .position(|jamo| *jamo == character)
                .map(|index| MEDIAL_KEYS[index])
        })
        .or_else(|| {
            FINALS
                .iter()
                .position(|jamo| *jamo == character)
                .map(|index| FINAL_KEYS[index])
        })
        .or_else(|| conjoining_jamo_to_keys(character))
}

fn conjoining_jamo_to_keys(character: char) -> Option<&'static str> {
    let code = character as u32;

    if (0x1100..=0x1112).contains(&code) {
        return Some(LEAD_KEYS[(code - 0x1100) as usize]);
    }
    if (0x1161..=0x1175).contains(&code) {
        return Some(MEDIAL_KEYS[(code - 0x1161) as usize]);
    }
    if (0x11A8..=0x11C2).contains(&code) {
        return Some(FINAL_KEYS[(code - 0x11A7) as usize]);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_english_keys_to_composed_korean() {
        assert_eq!(convert_to_korean("dkssudgktpdy"), "안녕하세요");
        assert_eq!(convert_to_korean("gksrmf"), "한글");
    }

    #[test]
    fn converts_korean_syllables_to_english_keys() {
        assert_eq!(convert_to_english("안녕하세요"), "dkssudgktpdy");
        assert_eq!(convert_to_english("한글"), "gksrmf");
    }

    #[test]
    fn converts_mixed_text_in_one_explicit_direction() {
        assert_eq!(
            convert_to_english("안녕 hello 테스트"),
            "dkssud hello xptmxm"
        );
        assert_eq!(
            convert_to_korean("dkssud hello xptmxm"),
            "안녕 ㅗ디ㅣㅐ 테스트"
        );
    }

    #[test]
    fn composes_compound_vowels_and_final_clusters() {
        assert_eq!(convert_to_korean("rhkd"), "광");
        assert_eq!(convert_to_korean("rkqt"), "값");
        assert_eq!(convert_to_korean("ekfr"), "닭");
    }

    #[test]
    fn moves_a_final_consonant_to_the_next_syllable_before_a_vowel() {
        assert_eq!(convert_to_korean("rksk"), "가나");
        assert_eq!(convert_to_korean("ekfrk"), "달가");
        assert_eq!(convert_to_korean("ekfrdk"), "닭아");
    }

    #[test]
    fn decomposes_compound_vowels_and_final_clusters() {
        assert_eq!(convert_to_english("광값닭"), "rhkdrkqtekfr");
    }

    #[test]
    fn converts_compatibility_and_conjoining_jamo() {
        assert_eq!(convert_to_english("ㄱㅏㄳㅘ"), "rkrt hk".replace(' ', ""));
        assert_eq!(convert_to_english("각"), "rkr");
    }

    #[test]
    fn supports_shifted_and_unshifted_uppercase_keys() {
        assert_eq!(convert_to_korean("Rk Eo Qo To Wo"), "까 때 빼 쌔 째");
        assert_eq!(convert_to_korean("DKSSUD"), "안녕");
    }

    #[test]
    fn preserves_numbers_symbols_whitespace_and_line_breaks() {
        let input = "123 !@#$%^&*()\n\tdkssud?";
        assert_eq!(convert_to_korean(input), "123 !@#$%^&*()\n\t안녕?");
        assert_eq!(convert_to_english("123 안녕!\n\t"), "123 dkssud!\n\t");
    }

    #[test]
    fn handles_empty_strings() {
        assert_eq!(convert_to_korean(""), "");
        assert_eq!(convert_to_english(""), "");
    }

    #[test]
    fn preserves_unconvertible_unicode() {
        let input = "漢字🙂 café";
        assert_eq!(convert_to_english(input), input);
        assert_eq!(convert_to_korean("漢字🙂 42"), "漢字🙂 42");
    }

    #[test]
    fn explicit_direction_api_matches_named_functions() {
        assert_eq!(
            convert("dkssud", ConversionDirection::EnglishToKorean),
            convert_to_korean("dkssud")
        );
        assert_eq!(
            convert("안녕", ConversionDirection::KoreanToEnglish),
            convert_to_english("안녕")
        );
    }

    #[test]
    fn conversion_is_deterministic() {
        let input = "dkssud hello xptmx\n123🙂";
        let first = convert_to_korean(input);
        assert_eq!(first, convert_to_korean(input));
        assert_eq!(
            convert_to_english("안녕 hello 테스트"),
            convert_to_english("안녕 hello 테스트")
        );
    }

    #[test]
    fn every_modern_hangul_syllable_round_trips_through_key_sequences() {
        for code in HANGUL_BASE..=HANGUL_END {
            let syllable = char::from_u32(code).unwrap();
            let keys = convert_to_english(&syllable.to_string());
            assert_eq!(
                convert_to_korean(&keys),
                syllable.to_string(),
                "failed to round-trip U+{code:04X} through {keys:?}"
            );
        }
    }
}
