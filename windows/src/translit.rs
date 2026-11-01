// Indic scripts -> English letters (the "Tanglish / Manglish" way people
// type lyrics), done locally with no network and no tables in memory beyond
// these matches. Devanagari, Bengali, Gurmukhi, Gujarati, Odia, Tamil,
// Telugu, Kannada and Malayalam share one Unicode layout (each a 128-char
// block starting at U+0900 .. U+0D00), so one offset map covers them all.

fn block(c: char) -> Option<(u32, u32)> {
    let u = c as u32;
    if (0x0900..0x0D80).contains(&u) {
        Some(((u - 0x0900) / 0x80, (u - 0x0900) % 0x80))
    } else {
        None
    }
}

const DEVANAGARI: u32 = 0;
const MALAYALAM: u32 = 8;

fn consonant(off: u32) -> Option<&'static str> {
    Some(match off {
        0x15 => "k", 0x16 => "kh", 0x17 => "g", 0x18 => "gh", 0x19 => "ng",
        0x1A => "ch", 0x1B => "chh", 0x1C => "j", 0x1D => "jh", 0x1E => "nj",
        0x1F => "t", 0x20 => "th", 0x21 => "d", 0x22 => "dh", 0x23 => "n",
        0x24 => "th", 0x25 => "thh", 0x26 => "d", 0x27 => "dh", 0x28 => "n", 0x29 => "n",
        0x2A => "p", 0x2B => "ph", 0x2C => "b", 0x2D => "bh", 0x2E => "m",
        0x2F => "y", 0x30 => "r", 0x31 => "r", 0x32 => "l", 0x33 => "l", 0x34 => "zh",
        0x35 => "v", 0x36 => "sh", 0x37 => "sh", 0x38 => "s", 0x39 => "h",
        0x58 => "q", 0x59 => "kh", 0x5A => "gh", 0x5B => "z", 0x5C => "d", 0x5D => "rh", 0x5E => "f", 0x5F => "y",
        _ => return None,
    })
}

fn vowel(off: u32) -> Option<&'static str> {
    Some(match off {
        0x05 => "a", 0x06 => "aa", 0x07 => "i", 0x08 => "ee", 0x09 => "u", 0x0A => "oo",
        0x0B => "ru", 0x0C => "lu", 0x0D | 0x0E | 0x0F => "e", 0x10 => "ai",
        0x11 | 0x12 | 0x13 => "o", 0x14 => "au", 0x60 => "roo", 0x61 => "loo",
        _ => return None,
    })
}

fn vowel_sign(off: u32) -> Option<&'static str> {
    Some(match off {
        0x3E => "aa", 0x3F => "i", 0x40 => "ee", 0x41 => "u", 0x42 => "oo",
        0x43 => "ru", 0x44 => "roo", 0x45 | 0x46 | 0x47 => "e", 0x48 => "ai",
        0x49 | 0x4A | 0x4B => "o", 0x4C | 0x57 => "au", 0x62 => "lu", 0x63 => "loo",
        _ => return None,
    })
}

// Malayalam chillu letters (consonants with no vowel).
fn chillu(off: u32) -> Option<&'static str> {
    Some(match off {
        0x54 => "m", 0x55 => "y", 0x56 => "zh", 0x7A => "n", 0x7B => "n", 0x7C => "r",
        0x7D => "l", 0x7E => "l", 0x7F => "k",
        _ => return None,
    })
}

pub fn has_indic(s: &str) -> bool {
    s.chars().any(|c| block(c).is_some())
}

pub fn to_latin(s: &str) -> String {
    if !has_indic(s) {
        return s.to_string();
    }
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    let mut half: u32 = 0; // consonant just closed by a virama
    while i < chars.len() {
        let c = chars[i];
        let Some((script, off)) = block(c) else {
            out.push(c);
            i += 1;
            continue;
        };
        if let Some(cons) = consonant(off) {
            // Malayalam ന്റ = "nt", റ്റ = "tt"
            let cons = if script == MALAYALAM && off == 0x31 && (half == 0x28 || half == 0x31) {
                if half == 0x31 { out.pop(); out.push('t'); }
                "t"
            } else { cons };
            half = 0;
            out.push_str(cons);
            let mut j = i + 1;
            // nukta
            if j < chars.len() && block(chars[j]).map(|b| b.1) == Some(0x3C) {
                j += 1;
            }
            let next = chars.get(j).and_then(|n| block(*n));
            match next {
                Some((_, o)) if vowel_sign(o).is_some() => {
                    out.push_str(vowel_sign(o).unwrap());
                    j += 1;
                }
                Some((_, 0x4D)) => {
                    j += 1;
                    half = off;
                    // Malayalam word-final virama is said as a short "u"
                    let word_end = chars.get(j).map(|n| block(*n).is_none()).unwrap_or(true);
                    if script == MALAYALAM && word_end {
                        out.push('u');
                    }
                }
                _ => {
                    // Hindi drops the final "a" of a word
                    let word_end = next.is_none() || matches!(next, Some((_, o)) if o < 0x05);
                    let word_end = word_end && !matches!(next, Some((_, 0x01..=0x03)));
                    if !(script == DEVANAGARI && word_end && chars.get(j).map(|n| !n.is_alphanumeric()).unwrap_or(true)) {
                        out.push('a');
                    }
                }
            }
            i = j;
            continue;
        }
        half = 0;
        if let Some(v) = vowel(off) {
            out.push_str(v);
        } else if let Some(v) = vowel_sign(off) {
            out.push_str(v);
        } else if let Some(ch) = chillu(off) {
            out.push_str(ch);
        } else {
            match off {
                0x01 => out.push('n'),
                0x02 => out.push(if script == DEVANAGARI { 'n' } else { 'm' }),
                0x03 => out.push('h'),
                0x64 | 0x65 => out.push('.'),
                0x66..=0x6F => out.push(char::from_digit(off - 0x66, 10).unwrap_or('0')),
                _ => {}
            }
        }
        i += 1;
    }
    // Capitalise the first letter of the line
    let mut it = out.chars();
    match it.next() {
        Some(f) => f.to_uppercase().collect::<String>() + it.as_str(),
        None => out,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tamil_malayalam_hindi() {
        assert_eq!(to_latin("வணக்கம்"), "Vanakkam");
        assert_eq!(to_latin("നമസ്കാരം"), "Namaskaaram");
        assert_eq!(to_latin("दिल"), "Dil");
        assert_eq!(to_latin("hello"), "hello");
    }
}
