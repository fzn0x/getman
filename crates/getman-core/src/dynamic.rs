//! Dynamic values: `{{$uuid}}`, `{{$timestamp}}`, `{{$randomEmail}}` and
//! friends. Every reference produces a fresh value.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::time::iso8601;

/// The dynamic values Getman knows, with a short description of each.
pub const NAMES: &[(&str, &str)] = &[
    ("$uuid", "A random UUID v4"),
    ("$guid", "A random UUID v4"),
    ("$randomUUID", "A random UUID v4"),
    ("$timestamp", "Current Unix time in seconds"),
    ("$timestampMs", "Current Unix time in milliseconds"),
    ("$isoTimestamp", "Current time in ISO 8601, UTC"),
    ("$randomInt", "A whole number from 0 to 1000"),
    ("$randomBoolean", "true or false"),
    ("$randomAlphaNumeric", "One random letter or digit"),
    ("$randomHexadecimal", "One random hexadecimal digit"),
    ("$randomFirstName", "A first name"),
    ("$randomLastName", "A last name"),
    ("$randomFullName", "A first and last name"),
    ("$randomUserName", "A user name"),
    ("$randomEmail", "An email address at example.com"),
    ("$randomPassword", "A 16-character password"),
    ("$randomPhoneNumber", "A phone number like 555-0142-3381"),
    ("$randomCity", "A city name"),
    ("$randomCountry", "A country name"),
    ("$randomCountryCode", "A two-letter country code"),
    ("$randomColor", "A color name"),
    ("$randomHexColor", "A color like #3fa2c4"),
    ("$randomWord", "A random word"),
    ("$randomWords", "A few random words"),
    ("$randomLoremSentence", "A sentence of placeholder text"),
    ("$randomIP", "An IPv4 address"),
    ("$randomIPV6", "An IPv6 address"),
    ("$randomMACAddress", "A MAC address"),
    ("$randomDomainName", "A domain name under example"),
    ("$randomUrl", "A URL under example.com"),
    ("$randomPrice", "A price such as 42.17"),
    ("$randomDatePast", "A date in the past year, ISO 8601"),
    ("$randomDateFuture", "A date in the next year, ISO 8601"),
];

const FIRST_NAMES: &[&str] = &[
    "Ada", "Alan", "Amara", "Ana", "Arjun", "Aziz", "Bea", "Chen", "Dara", "Diego", "Elif", "Emil",
    "Fatima", "Grace", "Hana", "Ines", "Ivan", "Jae", "Kofi", "Lena", "Linus", "Maya", "Nadia",
    "Noor", "Omar", "Priya", "Rafael", "Sanjay", "Sofia", "Tariq", "Uma", "Yuki", "Zara",
];
const LAST_NAMES: &[&str] = &[
    "Adeyemi", "Barros", "Costa", "Dubois", "Eriksen", "Fischer", "Garcia", "Haddad", "Ito",
    "Jensen", "Kowalski", "Larsen", "Mendes", "Nakamura", "Okafor", "Park", "Quinn", "Rossi",
    "Santoso", "Tanaka", "Usman", "Varga", "Wibowo", "Xu", "Yilmaz", "Zhang",
];
const CITIES: &[&str] = &[
    "Accra",
    "Bandung",
    "Bogotá",
    "Cairo",
    "Dublin",
    "Hanoi",
    "Helsinki",
    "Kyoto",
    "Lagos",
    "Lima",
    "Lisbon",
    "Montreal",
    "Nairobi",
    "Osaka",
    "Porto",
    "Seoul",
    "Surabaya",
    "Tbilisi",
    "Valencia",
    "Yogyakarta",
];
const COUNTRIES: &[(&str, &str)] = &[
    ("Argentina", "AR"),
    ("Brazil", "BR"),
    ("Canada", "CA"),
    ("Egypt", "EG"),
    ("Finland", "FI"),
    ("Ghana", "GH"),
    ("India", "IN"),
    ("Indonesia", "ID"),
    ("Ireland", "IE"),
    ("Japan", "JP"),
    ("Kenya", "KE"),
    ("Mexico", "MX"),
    ("Nigeria", "NG"),
    ("Portugal", "PT"),
    ("South Korea", "KR"),
    ("Spain", "ES"),
    ("Turkey", "TR"),
    ("Vietnam", "VN"),
];
const COLORS: &[&str] = &[
    "amber", "azure", "coral", "crimson", "indigo", "ivory", "jade", "lavender", "lime", "magenta",
    "olive", "orchid", "plum", "saffron", "teal", "violet",
];
const WORDS: &[&str] = &[
    "anchor", "basket", "candle", "delta", "ember", "fable", "garden", "harbor", "island",
    "jigsaw", "kettle", "lantern", "meadow", "nectar", "orbit", "pebble", "quartz", "river",
    "saddle", "timber", "umbrella", "velvet", "willow", "yonder", "zephyr", "bridge", "cobalt",
    "drift", "echo", "falcon", "glacier", "horizon", "lagoon", "mosaic", "nimbus", "prism",
];
const LOREM: &[&str] = &[
    "lorem",
    "ipsum",
    "dolor",
    "sit",
    "amet",
    "consectetur",
    "adipiscing",
    "elit",
    "sed",
    "do",
    "eiusmod",
    "tempor",
    "incididunt",
    "ut",
    "labore",
    "et",
    "dolore",
    "magna",
    "aliqua",
];

/// Produces the value of `${name}`, or `None` if Getman has no such
/// dynamic value.
pub fn generate(name: &str) -> Option<String> {
    let value = match name {
        "uuid" | "guid" | "randomUUID" => uuid_v4(),
        "timestamp" => now().as_secs().to_string(),
        "timestampMs" => now().as_millis().to_string(),
        "isoTimestamp" => iso8601(SystemTime::now()),
        "randomInt" => fastrand::u32(0..=1000).to_string(),
        "randomBoolean" => fastrand::bool().to_string(),
        "randomAlphaNumeric" => fastrand::alphanumeric().to_ascii_lowercase().to_string(),
        "randomHexadecimal" => format!("{:x}", fastrand::u8(0..16)),
        "randomFirstName" => pick(FIRST_NAMES).to_owned(),
        "randomLastName" => pick(LAST_NAMES).to_owned(),
        "randomFullName" => format!("{} {}", pick(FIRST_NAMES), pick(LAST_NAMES)),
        "randomUserName" => user_name(),
        "randomEmail" => format!("{}@example.com", user_name()),
        "randomPassword" => std::iter::repeat_with(fastrand::alphanumeric)
            .take(16)
            .collect(),
        "randomPhoneNumber" => format!(
            "555-{:04}-{:04}",
            fastrand::u16(100..200),
            fastrand::u16(0..10_000)
        ),
        "randomCity" => pick(CITIES).to_owned(),
        "randomCountry" => pick(COUNTRIES).0.to_owned(),
        "randomCountryCode" => pick(COUNTRIES).1.to_owned(),
        "randomColor" => pick(COLORS).to_owned(),
        "randomHexColor" => format!("#{:06x}", fastrand::u32(0..0x0100_0000)),
        "randomWord" => pick(WORDS).to_owned(),
        "randomWords" => words(WORDS, fastrand::usize(2..5)),
        "randomLoremSentence" => {
            let mut sentence = words(LOREM, fastrand::usize(5..10));
            if let Some(first) = sentence.get_mut(..1) {
                first.make_ascii_uppercase();
            }
            sentence.push('.');
            sentence
        }
        "randomIP" => format!(
            "{}.{}.{}.{}",
            fastrand::u8(1..=254),
            fastrand::u8(..),
            fastrand::u8(..),
            fastrand::u8(1..=254)
        ),
        "randomIPV6" => (0..8)
            .map(|_| format!("{:x}", fastrand::u16(..)))
            .collect::<Vec<_>>()
            .join(":"),
        "randomMACAddress" => (0..6)
            .map(|_| format!("{:02x}", fastrand::u8(..)))
            .collect::<Vec<_>>()
            .join(":"),
        "randomDomainName" => format!("{}.example", pick(WORDS)),
        "randomUrl" => format!("https://{}.example.com/{}", pick(WORDS), pick(WORDS)),
        "randomPrice" => format!("{}.{:02}", fastrand::u32(1..1000), fastrand::u8(0..100)),
        "randomDatePast" => {
            iso8601(SystemTime::now() - Duration::from_secs(fastrand::u64(1..365 * 86_400)))
        }
        "randomDateFuture" => {
            iso8601(SystemTime::now() + Duration::from_secs(fastrand::u64(1..365 * 86_400)))
        }
        _ => return None,
    };
    Some(value)
}

/// A random UUID, version 4.
pub fn uuid_v4() -> String {
    let mut bytes = fastrand::u128(..).to_be_bytes();
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    )
}

fn now() -> Duration {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
}

fn pick<T: Copy>(items: &[T]) -> T {
    items[fastrand::usize(..items.len())]
}

fn words(list: &[&str], count: usize) -> String {
    (0..count).map(|_| pick(list)).collect::<Vec<_>>().join(" ")
}

fn user_name() -> String {
    format!(
        "{}.{}{}",
        pick(FIRST_NAMES).to_lowercase(),
        pick(LAST_NAMES).to_lowercase(),
        fastrand::u8(1..100)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_name_generates_a_value() {
        for (name, _) in NAMES {
            let value = generate(&name[1..]).unwrap_or_else(|| panic!("{name}"));
            assert!(!value.is_empty(), "{name}");
        }
        assert_eq!(generate("nope"), None);
    }

    #[test]
    fn uuids_have_the_version_and_variant_bits() {
        let id = uuid_v4();
        assert_eq!(id.len(), 36);
        assert_eq!(&id[14..15], "4");
        assert!(matches!(&id[19..20], "8" | "9" | "a" | "b"), "{id}");
    }

    #[test]
    fn sentences_start_with_a_capital_and_end_with_a_period() {
        let sentence = generate("randomLoremSentence").unwrap();
        assert!(sentence.starts_with(|c: char| c.is_ascii_uppercase()));
        assert!(sentence.ends_with('.'));
    }
}
