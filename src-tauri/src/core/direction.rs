use crate::converter::ConversionDirection;

pub(super) fn choose_direction(text: &str) -> Option<ConversionDirection> {
    if text.is_empty() {
        return None;
    }

    Some(if text.chars().any(is_hangul) {
        ConversionDirection::KoreanToEnglish
    } else {
        ConversionDirection::EnglishToKorean
    })
}

fn is_hangul(character: char) -> bool {
    matches!(
        character as u32,
        0x1100..=0x11FF | 0x3130..=0x318F | 0xA960..=0xA97F | 0xAC00..=0xD7A3 | 0xD7B0..=0xD7FF
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_has_no_direction() {
        assert_eq!(choose_direction(""), None);
    }

    #[test]
    fn english_only_chooses_english_to_korean() {
        assert_eq!(
            choose_direction("dkssud hello 123"),
            Some(ConversionDirection::EnglishToKorean)
        );
    }

    #[test]
    fn korean_containing_mixed_text_chooses_korean_to_english() {
        assert_eq!(
            choose_direction("안녕 hello 테스트"),
            Some(ConversionDirection::KoreanToEnglish)
        );
        assert_eq!(
            choose_direction("ᄀ and ㄱ"),
            Some(ConversionDirection::KoreanToEnglish)
        );
    }
}
