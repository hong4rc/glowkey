//! A curated list of common English words for the opt-in "restore English words"
//! feature (see `Session::commit`).
//!
//! Auto-fix already restores any word whose Telex rendering is *invalid*
//! Vietnamese (`exit`, `work`). The residue is words whose rendering happens to be
//! a **valid** syllable — `was`→`ứa`, `how`→`hơ`, `cats`→`cát` — where the engine
//! cannot tell English from Vietnamese. This list resolves that ambiguity in favor
//! of English *when the user opts in*: with the option on, a committed word whose
//! raw keys match an entry is restored to those keys even if the rendering is
//! valid Vietnamese.
//!
//! The trade-off is inherent and cuts both ways: with the option on, Vietnamese
//! words normally typed with a trailing tone key (`cats`→`cát`, `car`→`cả`,
//! `hair`→`hải`) need a different key order or an EN toggle. That is why the
//! option defaults to off and why this list is curated rather than a full
//! dictionary — it contains only common English words, biased toward the ones
//! that actually transform under Telex (contain `w`, a tone letter after a vowel,
//! a double vowel, or `dd`). Entries are lowercase; matching is case-insensitive
//! on the raw keys. Lookup is a linear scan at word boundaries only, which is
//! nothing at this size.

/// Whether `raw` (the word's raw keystrokes) is a listed common English word.
#[must_use]
pub(crate) fn is_common_english(raw: &str) -> bool {
    let lowered = raw.to_ascii_lowercase();
    WORDS.contains(&lowered.as_str())
}

/// Common English words. Lowercase. See the module docs for inclusion criteria.
///
/// Hand-packed and kept that way: rustfmt would put each word on its own line,
/// turning a list you can scan by theme into four hundred lines you cannot.
#[rustfmt::skip]
static WORDS: &[&str] = &[
    // Short function words whose rendering is valid Vietnamese.
    "as", "is", "us", "if", "of", "off", "or", "was", "has", "his", "hers", "its", "this",
    "thus", "yes", "gas", "bus", "plus", "whose",
    // -ss and -s clusters.
    "miss", "kiss", "less", "mess", "loss", "boss", "toss", "pass", "mass", "class",
    "glass", "grass", "press", "dress", "cross", "gross", "guess", "bless", "chess",
    "stress", "access", "across", "unless", "business", "process", "address",
    // w-initial words.
    "wage", "wait", "wake", "walk", "wall", "want", "wants", "war", "warm", "warn",
    "wash", "waste", "watch", "water", "wave", "way", "ways", "we", "weak", "wear",
    "weather", "web", "wedding", "week", "weeks", "weekend", "weight", "welcome",
    "well", "went", "were", "west", "wet", "what", "whatever", "wheel", "when", "where",
    "whether", "which", "while", "white", "who", "whole", "whom", "why", "wide", "wife",
    "wild", "will", "win", "wins", "wind", "window", "windows", "wine", "wing",
    "winner", "winter", "wire", "wise", "wish", "with", "within", "without", "woman",
    "women", "wonder", "wonderful", "wood", "wool", "word", "words", "wore", "work",
    "worked", "worker", "works", "world", "worry", "worse", "worst", "worth", "would",
    "wound", "wrap", "write", "writes", "writing", "wrong", "wrote",
    // w elsewhere.
    "two", "twelve", "twenty", "twice", "between", "answer", "away", "awake", "aware",
    "award", "awesome", "awful", "forward", "backward", "toward", "towards", "however",
    "power", "powers", "flower", "shower", "tower", "towel", "owner", "own", "owns",
    "owned", "owe", "owes", "owl", "few", "new", "news", "view", "review", "interview",
    // -ow words (ow→ơ renders valid Vietnamese: how→hơ, now→nơ).
    "allow", "allows", "arrow", "below", "blow", "borrow", "bow", "brown", "cow",
    "crow", "crowd", "crown", "down", "download", "elbow", "fellow", "flow", "flows",
    "follow", "follows", "glow", "gown", "grow", "grows", "grown", "how", "know",
    "knows", "known", "low", "lower", "narrow", "now", "pillow", "rainbow", "row",
    "shadow", "show", "shows", "shown", "slow", "snow", "sorrow", "throw", "throws",
    "thrown", "tomorrow", "town", "vow", "wow", "yellow",
    // Double vowels (aa/ee/oo are Telex diacritics).
    "agree", "been", "bee", "bees", "beer", "beef", "cheese", "coffee", "cool", "deep",
    "degree", "door", "feel", "feels", "feet", "fee", "fees", "floor", "food", "foot",
    "free", "freedom", "good", "goods", "goose", "green", "indeed", "keep", "keeps",
    "look", "looks", "loose", "meet", "meets", "meeting", "moon", "mood", "need",
    "needs", "noon", "pool", "poor", "queen", "room", "rooms", "roof", "root",
    "school", "screen", "see", "sees", "seen", "seem", "seems", "sheep", "sheet",
    "sleep", "smooth", "soon", "speed", "spoon", "street", "sweet", "teen", "three",
    "too", "took", "tool", "tools", "tooth", "tree", "trees", "wheels", "zoo", "book",
    "books", "blood", "flood", "choose", "cook", "cooks", "cookie",
    // dd (Telex đ).
    "add", "adds", "added", "ladder", "middle", "sudden", "suddenly", "odd", "odds",
    "hidden", "riddle",
    // Plural/3rd-person -s that renders a valid tone (cats→cát, tips→típ, bus→bú).
    "acts", "asks", "bags", "bats", "bets", "bits", "boys", "buns", "buts", "calls",
    "cans", "caps", "cars", "cats", "chips", "comes", "cops", "costs", "cups", "cuts",
    "days", "does", "dogs", "dots", "eats", "ends", "eyes", "fans", "finds", "fits",
    "gets", "gives", "goes", "guns", "guys", "hands", "hats", "helps", "hits", "hopes",
    "hours", "jobs", "keys", "kids", "kits", "knees", "laps", "lets", "likes", "lips",
    "lives", "lots", "loves", "makes", "maps", "means", "minds", "moms", "moves",
    "names", "naps", "nets", "notes", "nuts", "opens", "pans", "pants", "pens", "pets",
    "pins", "plans", "plants", "plays", "points", "pots", "puts", "rats", "reads",
    "runs", "says", "seats", "sets", "ships", "shirts", "shoes", "shops", "shots",
    "sides", "sits", "sons", "sounds", "speaks", "sports", "stands", "starts", "stays",
    "steps", "stops", "suns", "takes", "talks", "tanks", "taps", "tells", "tens",
    "tents", "tests", "thanks", "things", "thinks", "tins", "tips", "tons", "tops",
    "toys", "turns", "uses", "vans", "brings", "buys", "pays", "tries", "flies",
    // Tone-letter finals after a vowel (r→hỏi, x→ngã, s→sắc, f→huyền, j→nặng).
    "air", "bar", "car", "chair", "clear", "dear", "ear", "far", "fear", "four", "fur",
    "hair", "hear", "hour", "jar", "near", "our", "ours", "pair", "sir", "sour",
    "star", "stir", "tar", "tour", "year", "years", "your", "yours",
    "box", "fax", "fix", "fox", "max", "mix", "six", "tax", "wax", "relax", "next",
    "text", "exit", "expect", "example", "exam",
    // f mid-word (f is the huyền tone key).
    "after", "offer", "office", "often", "safe", "life", "left", "soft", "gift",
    "self", "half", "staff", "stuff", "effort", "afford", "different",
    // r mid-word (r is the hỏi tone key) — common ones only.
    "are", "very", "every", "here", "there", "more", "sure", "fire", "care", "share",
    "before", "store",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_case_insensitively() {
        assert!(is_common_english("was"));
        assert!(is_common_english("Was"));
        assert!(is_common_english("WAS"));
        assert!(!is_common_english("hoongf"));
        assert!(!is_common_english(""));
    }

    #[test]
    fn no_duplicate_entries() {
        let mut seen = std::collections::BTreeSet::new();
        for w in WORDS {
            assert!(seen.insert(*w), "duplicate wordlist entry: {w}");
        }
    }

    #[test]
    fn entries_are_lowercase_ascii_letters() {
        for w in WORDS {
            assert!(
                !w.is_empty() && w.chars().all(|c| c.is_ascii_lowercase()),
                "bad wordlist entry: {w:?}"
            );
        }
    }
}
