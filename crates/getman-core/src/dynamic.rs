//! Dynamic values: `{{$uuid}}`, `{{$timestamp}}`, `{{$randomEmail}}` and
//! friends. Every reference produces a fresh value.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::time::iso8601;

mod data;

use data::{
    ABBREVIATIONS, ACCOUNT_NAMES, ADJECTIVES, BS_ADJECTIVES, BS_BUZZ, BS_NOUNS, CATCH_ADJECTIVES,
    CATCH_DESCRIPTORS, CATCH_NOUNS, COMMON_EXTENSIONS, COMPANY_SUFFIXES, CURRENCIES,
    DATABASE_COLLATIONS, DATABASE_COLUMNS, DATABASE_ENGINES, DATABASE_TYPES, DEPARTMENTS,
    DIRECTORIES, DOMAIN_SUFFIXES, EXAMPLE_DOMAINS, FILE_TYPES, IMAGE_KINDS, ING_VERBS, JOB_AREAS,
    JOB_DESCRIPTORS, JOB_TYPES, LOCALES, LOREM_WORDS, MONTHS, NAME_PREFIXES, NAME_SUFFIXES, NOUNS,
    PHRASES, PRODUCT_ADJECTIVES, PRODUCT_MATERIALS, PRODUCTS, STREET_KINDS, TRANSACTION_TYPES,
    USER_AGENT_PLATFORMS, VERBS, WEEKDAYS, choose,
};

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
    (
        "$randomAbbreviation",
        "A technical abbreviation such as TCP",
    ),
    ("$randomLocale", "A locale code such as pt-BR"),
    ("$randomSemver", "A version number such as 3.14.2"),
    ("$randomUserAgent", "A browser User-Agent string"),
    ("$randomProtocol", "http or https"),
    ("$randomDomainSuffix", "A domain suffix such as com or dev"),
    ("$randomDomainWord", "A single lowercase word for a domain"),
    (
        "$randomExampleEmail",
        "An email address at example.com, .net or .org",
    ),
    ("$randomNamePrefix", "A title such as Dr"),
    ("$randomNameSuffix", "A name suffix such as Jr"),
    (
        "$randomPhoneNumberExt",
        "A phone number with a country prefix, like 94-315-425-6711",
    ),
    ("$randomStreetName", "A street name"),
    ("$randomStreetAddress", "A house number and street name"),
    ("$randomLatitude", "A latitude from -90 to 90"),
    ("$randomLongitude", "A longitude from -180 to 180"),
    ("$randomJobArea", "A job area such as Logistics"),
    ("$randomJobDescriptor", "A job descriptor such as Senior"),
    ("$randomJobType", "A job type such as Analyst"),
    (
        "$randomJobTitle",
        "A job title such as Senior Logistics Analyst",
    ),
    ("$randomCompanyName", "A company name"),
    ("$randomCompanySuffix", "A company suffix such as LLC"),
    ("$randomBs", "Business jargon such as pivot tiered funnels"),
    ("$randomBsAdjective", "A business jargon adjective"),
    ("$randomBsBuzz", "A business jargon verb"),
    ("$randomBsNoun", "A business jargon noun"),
    ("$randomCatchPhrase", "A marketing slogan"),
    ("$randomCatchPhraseAdjective", "A slogan adjective"),
    ("$randomCatchPhraseDescriptor", "A slogan descriptor"),
    ("$randomCatchPhraseNoun", "A slogan noun"),
    ("$randomDateRecent", "A date within the last day, ISO 8601"),
    ("$randomWeekday", "A day of the week"),
    ("$randomMonth", "A month name"),
    ("$randomCurrencyCode", "A currency code such as EUR"),
    ("$randomCurrencyName", "A currency name such as Euro"),
    ("$randomCurrencySymbol", "A currency symbol such as $"),
    ("$randomBankAccount", "An 8-digit bank account number"),
    (
        "$randomBankAccountName",
        "A bank account name such as Savings Account",
    ),
    ("$randomBankAccountBic", "A BIC code such as ABCDGB2L"),
    ("$randomBankAccountIban", "An IBAN with a valid check digit"),
    ("$randomCreditCardMask", "The last four digits of a card"),
    (
        "$randomTransactionType",
        "A transaction type such as deposit",
    ),
    ("$randomBitcoin", "A Bitcoin address"),
    ("$randomDatabaseColumn", "A database column name"),
    ("$randomDatabaseType", "A database column type"),
    ("$randomDatabaseCollation", "A database collation"),
    ("$randomDatabaseEngine", "A database engine"),
    ("$randomDepartment", "A shop department"),
    ("$randomProductAdjective", "A product adjective"),
    ("$randomProductMaterial", "A product material"),
    ("$randomProduct", "A product such as Kettle"),
    (
        "$randomProductName",
        "A product name such as Sleek Oak Kettle",
    ),
    ("$randomNoun", "A noun"),
    ("$randomVerb", "A verb"),
    ("$randomIngverb", "A verb ending in -ing"),
    ("$randomAdjective", "An adjective"),
    ("$randomPhrase", "A short sentence"),
    ("$randomLoremWord", "One word of placeholder text"),
    ("$randomLoremWords", "Three words of placeholder text"),
    (
        "$randomLoremSentences",
        "Two to five sentences of placeholder text",
    ),
    ("$randomLoremParagraph", "A paragraph of placeholder text"),
    ("$randomLoremParagraphs", "Three paragraphs, one per line"),
    ("$randomLoremText", "Placeholder text of a random length"),
    (
        "$randomLoremSlug",
        "Three placeholder words joined by dashes",
    ),
    ("$randomLoremLines", "One to four lines of placeholder text"),
    ("$randomFileName", "A file name such as lantern.pdf"),
    ("$randomFileExt", "A file extension such as pdf"),
    ("$randomFileType", "A file type such as image or audio"),
    ("$randomMimeType", "A MIME type such as image/png"),
    (
        "$randomFilePath",
        "A file path such as /var/log/lantern.txt",
    ),
    ("$randomDirectoryPath", "A directory path"),
    (
        "$randomCommonFileName",
        "A file name with a common extension",
    ),
    (
        "$randomCommonFileExt",
        "A common file extension such as json",
    ),
    (
        "$randomCommonFileType",
        "The file type of a common extension",
    ),
    (
        "$randomAvatarImage",
        "A URL for an avatar image under example.com",
    ),
    ("$randomImageUrl", "A URL for an image under example.com"),
    ("$randomAbstractImage", "A URL for an abstract image"),
    ("$randomAnimalsImage", "A URL for an animals image"),
    ("$randomBusinessImage", "A URL for a business image"),
    ("$randomCatsImage", "A URL for a cats image"),
    ("$randomCityImage", "A URL for a city image"),
    ("$randomFoodImage", "A URL for a food image"),
    ("$randomNightlifeImage", "A URL for a nightlife image"),
    ("$randomFashionImage", "A URL for a fashion image"),
    ("$randomPeopleImage", "A URL for a people image"),
    ("$randomNatureImage", "A URL for a nature image"),
    ("$randomSportsImage", "A URL for a sports image"),
    ("$randomTechnicsImage", "A URL for a technics image"),
    ("$randomTransportImage", "A URL for a transport image"),
    ("$randomImageDataUri", "A small SVG image as a data: URI"),
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
        "randomLoremSentence" => lorem_sentence(fastrand::usize(5..10)),
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
        "randomAbbreviation" => choose(ABBREVIATIONS).to_owned(),
        "randomLocale" => choose(LOCALES).to_owned(),
        "randomSemver" => format!(
            "{}.{}.{}",
            fastrand::u8(0..10),
            fastrand::u8(0..30),
            fastrand::u8(0..30)
        ),
        "randomUserAgent" => user_agent(),
        "randomProtocol" => choose("http,https").to_owned(),
        "randomDomainSuffix" => choose(DOMAIN_SUFFIXES).to_owned(),
        "randomDomainWord" => pick(WORDS).to_owned(),
        "randomExampleEmail" => format!("{}@{}", user_name(), choose(EXAMPLE_DOMAINS)),
        "randomNamePrefix" => choose(NAME_PREFIXES).to_owned(),
        "randomNameSuffix" => choose(NAME_SUFFIXES).to_owned(),
        "randomPhoneNumberExt" => format!(
            "{}-{}-{}-{:04}",
            fastrand::u8(1..100),
            fastrand::u16(100..1000),
            fastrand::u16(100..1000),
            fastrand::u16(0..10_000)
        ),
        "randomStreetName" => street_name(),
        "randomStreetAddress" => format!("{} {}", fastrand::u16(1..10_000), street_name()),
        "randomLatitude" => format!("{:.4}", fastrand::f64().mul_add(180.0, -90.0)),
        "randomLongitude" => format!("{:.4}", fastrand::f64().mul_add(360.0, -180.0)),
        "randomJobArea" => choose(JOB_AREAS).to_owned(),
        "randomJobDescriptor" => choose(JOB_DESCRIPTORS).to_owned(),
        "randomJobType" => choose(JOB_TYPES).to_owned(),
        "randomJobTitle" => format!(
            "{} {} {}",
            choose(JOB_DESCRIPTORS),
            choose(JOB_AREAS),
            choose(JOB_TYPES)
        ),
        "randomCompanyName" => company_name(),
        "randomCompanySuffix" => choose(COMPANY_SUFFIXES).to_owned(),
        "randomBs" => format!(
            "{} {} {}",
            choose(BS_BUZZ),
            choose(BS_ADJECTIVES),
            choose(BS_NOUNS)
        ),
        "randomBsAdjective" => choose(BS_ADJECTIVES).to_owned(),
        "randomBsBuzz" => choose(BS_BUZZ).to_owned(),
        "randomBsNoun" => choose(BS_NOUNS).to_owned(),
        "randomCatchPhrase" => format!(
            "{} {} {}",
            choose(CATCH_ADJECTIVES),
            choose(CATCH_DESCRIPTORS),
            choose(CATCH_NOUNS)
        ),
        "randomCatchPhraseAdjective" => choose(CATCH_ADJECTIVES).to_owned(),
        "randomCatchPhraseDescriptor" => choose(CATCH_DESCRIPTORS).to_owned(),
        "randomCatchPhraseNoun" => choose(CATCH_NOUNS).to_owned(),
        "randomDateRecent" => {
            iso8601(SystemTime::now() - Duration::from_secs(fastrand::u64(1..86_400)))
        }
        "randomWeekday" => choose(WEEKDAYS).to_owned(),
        "randomMonth" => choose(MONTHS).to_owned(),
        "randomCurrencyCode" => currency().0.to_owned(),
        "randomCurrencyName" => currency().1.to_owned(),
        "randomCurrencySymbol" => currency().2.to_owned(),
        "randomBankAccount" => digits(8),
        "randomBankAccountName" => choose(ACCOUNT_NAMES).to_owned(),
        "randomBankAccountBic" => bic(),
        "randomBankAccountIban" => iban(),
        "randomCreditCardMask" => digits(4),
        "randomTransactionType" => choose(TRANSACTION_TYPES).to_owned(),
        "randomBitcoin" => bitcoin_address(),
        "randomDatabaseColumn" => choose(DATABASE_COLUMNS).to_owned(),
        "randomDatabaseType" => choose(DATABASE_TYPES).to_owned(),
        "randomDatabaseCollation" => choose(DATABASE_COLLATIONS).to_owned(),
        "randomDatabaseEngine" => choose(DATABASE_ENGINES).to_owned(),
        "randomDepartment" => choose(DEPARTMENTS).to_owned(),
        "randomProductAdjective" => choose(PRODUCT_ADJECTIVES).to_owned(),
        "randomProductMaterial" => choose(PRODUCT_MATERIALS).to_owned(),
        "randomProduct" => choose(PRODUCTS).to_owned(),
        "randomProductName" => format!(
            "{} {} {}",
            choose(PRODUCT_ADJECTIVES),
            choose(PRODUCT_MATERIALS),
            choose(PRODUCTS)
        ),
        "randomNoun" => choose(NOUNS).to_owned(),
        "randomVerb" => choose(VERBS).to_owned(),
        "randomIngverb" => choose(ING_VERBS).to_owned(),
        "randomAdjective" => choose(ADJECTIVES).to_owned(),
        "randomPhrase" => phrase(),
        "randomLoremWord" => choose(LOREM_WORDS).to_owned(),
        "randomLoremWords" => lorem_words(3),
        "randomLoremSentences" => lorem_sentences(fastrand::usize(2..6)),
        "randomLoremParagraph" => lorem_sentences(fastrand::usize(3..7)),
        "randomLoremParagraphs" => (0..3)
            .map(|_| lorem_sentences(fastrand::usize(3..7)))
            .collect::<Vec<_>>()
            .join("\n"),
        "randomLoremText" => match fastrand::u8(0..4) {
            0 => lorem_words(fastrand::usize(3..8)),
            1 => lorem_sentence(fastrand::usize(5..10)),
            2 => lorem_sentences(fastrand::usize(2..6)),
            _ => lorem_sentences(fastrand::usize(4..8)),
        },
        "randomLoremSlug" => lorem_words(3).replace(' ', "-"),
        "randomLoremLines" => (0..fastrand::usize(1..5))
            .map(|_| lorem_sentence(fastrand::usize(5..10)))
            .collect::<Vec<_>>()
            .join("\n"),
        "randomFileName" => file_name(false),
        "randomFileExt" => file_type(false).0.to_owned(),
        "randomFileType" => file_type(false).2.to_owned(),
        "randomMimeType" => file_type(false).1.to_owned(),
        "randomFilePath" => format!("{}/{}", choose(DIRECTORIES), file_name(false)),
        "randomDirectoryPath" => choose(DIRECTORIES).to_owned(),
        "randomCommonFileName" => file_name(true),
        "randomCommonFileExt" => file_type(true).0.to_owned(),
        "randomCommonFileType" => file_type(true).2.to_owned(),
        "randomAvatarImage" => {
            format!("https://example.com/avatars/{}.jpg", fastrand::u16(1..1000))
        }
        "randomImageUrl" => image_url(choose(IMAGE_KINDS)),
        "randomAbstractImage" => image_url("abstract"),
        "randomAnimalsImage" => image_url("animals"),
        "randomBusinessImage" => image_url("business"),
        "randomCatsImage" => image_url("cats"),
        "randomCityImage" => image_url("city"),
        "randomFoodImage" => image_url("food"),
        "randomNightlifeImage" => image_url("nightlife"),
        "randomFashionImage" => image_url("fashion"),
        "randomPeopleImage" => image_url("people"),
        "randomNatureImage" => image_url("nature"),
        "randomSportsImage" => image_url("sports"),
        "randomTechnicsImage" => image_url("technics"),
        "randomTransportImage" => image_url("transport"),
        "randomImageDataUri" => format!(
            "data:image/svg+xml;charset=UTF-8,%3Csvg%20xmlns%3D%22http%3A%2F%2Fwww.w3.org%2F2000%2Fsvg%22%20width%3D%22100%22%20height%3D%22100%22%3E%3Crect%20width%3D%22100%22%20height%3D%22100%22%20fill%3D%22%23{:06x}%22%2F%3E%3C%2Fsvg%3E",
            fastrand::u32(0..0x0100_0000)
        ),
        _ => return None,
    };
    Some(value)
}

/// A random UUID, version 4, from the OS random generator: scripts use
/// `crypto.randomUUID` for idempotency keys and nonces, and the Web
/// Crypto API it stands in for promises an unpredictable value.
///
/// # Panics
///
/// If the operating system cannot supply random bytes.
pub fn uuid_v4() -> String {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).expect("the OS random number generator");
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

fn digits(count: usize) -> String {
    (0..count)
        .map(|_| char::from(b'0' + fastrand::u8(0..10)))
        .collect()
}

fn street_name() -> String {
    format!("{} {}", pick(LAST_NAMES), choose(STREET_KINDS))
}

fn company_name() -> String {
    match fastrand::u8(0..3) {
        0 => format!("{} {}", pick(LAST_NAMES), choose(COMPANY_SUFFIXES)),
        1 => format!("{} & {}", pick(LAST_NAMES), pick(LAST_NAMES)),
        _ => format!(
            "{}, {} and {}",
            pick(LAST_NAMES),
            pick(LAST_NAMES),
            pick(LAST_NAMES)
        ),
    }
}

/// Code, name and symbol of a currency.
fn currency() -> (&'static str, &'static str, &'static str) {
    let mut parts = choose(CURRENCIES).splitn(3, '|');
    (
        parts.next().unwrap_or_default(),
        parts.next().unwrap_or_default(),
        parts.next().unwrap_or_default(),
    )
}

/// Extension, MIME type and kind of a file type.
fn file_type(common: bool) -> (&'static str, &'static str, &'static str) {
    let entry = if common {
        let extension = choose(COMMON_EXTENSIONS);
        FILE_TYPES
            .split(',')
            .find(|entry| entry.split('|').next() == Some(extension))
            .unwrap_or_default()
    } else {
        choose(FILE_TYPES)
    };
    let mut parts = entry.splitn(3, '|');
    (
        parts.next().unwrap_or_default(),
        parts.next().unwrap_or_default(),
        parts.next().unwrap_or_default(),
    )
}

fn file_name(common: bool) -> String {
    format!("{}.{}", pick(WORDS), file_type(common).0)
}

fn image_url(kind: &str) -> String {
    format!(
        "https://example.com/images/{kind}/640x480.jpg?id={}",
        fastrand::u16(1..1000)
    )
}

fn user_agent() -> String {
    let platform = choose(USER_AGENT_PLATFORMS);
    if fastrand::bool() {
        format!(
            "Mozilla/5.0 ({platform}) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/{}.0.{}.{} Safari/537.36",
            fastrand::u8(118..132),
            fastrand::u16(4000..7000),
            fastrand::u8(0..200)
        )
    } else {
        let version = fastrand::u8(115..132);
        format!("Mozilla/5.0 ({platform}; rv:{version}.0) Gecko/20100101 Firefox/{version}.0")
    }
}

const BASE58: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

/// Looks like a Bitcoin address, but its checksum is not real.
fn bitcoin_address() -> String {
    let mut address = String::from(if fastrand::bool() { '1' } else { '3' });
    for _ in 0..fastrand::usize(25..34) {
        address.push(char::from(BASE58[fastrand::usize(..BASE58.len())]));
    }
    address
}

fn letters(count: usize) -> String {
    (0..count)
        .map(|_| char::from(b'A' + fastrand::u8(0..26)))
        .collect()
}

fn bic() -> String {
    let mut code = format!("{}{}", letters(4), pick(COUNTRIES).1);
    code.extend((0..2).map(|_| fastrand::alphanumeric().to_ascii_uppercase()));
    if fastrand::bool() {
        code.extend((0..3).map(|_| fastrand::alphanumeric().to_ascii_uppercase()));
    }
    code
}

/// An IBAN whose check digits are right, so validators accept it.
fn iban() -> String {
    // Country, length of the part after the check digits, and whether it
    // starts with a four-letter bank code.
    const IBANS: &[(&str, usize, bool)] = &[
        ("GB", 18, true),
        ("DE", 18, false),
        ("NL", 14, true),
        ("ES", 20, false),
    ];
    let (country, length, bank_code) = pick(IBANS);
    let mut national = if bank_code { letters(4) } else { String::new() };
    national.push_str(&digits(length - national.len()));
    // The check digits are 98 minus the remainder, modulo 97, of the number
    // made of the national part, the country and "00", with letters
    // turned into 10 to 35.
    let mut remainder = 0_u32;
    for c in national.chars().chain(country.chars()).chain("00".chars()) {
        let value = c.to_digit(36).unwrap_or(0);
        remainder = if value > 9 {
            (remainder * 100 + value) % 97
        } else {
            (remainder * 10 + value) % 97
        };
    }
    format!("{country}{:02}{national}", 98 - remainder)
}

fn lorem_words(count: usize) -> String {
    (0..count)
        .map(|_| choose(LOREM_WORDS))
        .collect::<Vec<_>>()
        .join(" ")
}

fn lorem_sentence(word_count: usize) -> String {
    let mut sentence = lorem_words(word_count);
    if let Some(first) = sentence.get_mut(..1) {
        first.make_ascii_uppercase();
    }
    sentence.push('.');
    sentence
}

fn lorem_sentences(count: usize) -> String {
    (0..count)
        .map(|_| lorem_sentence(fastrand::usize(5..10)))
        .collect::<Vec<_>>()
        .join(" ")
}

fn phrase() -> String {
    let mut text = choose(PHRASES).to_owned();
    // One placeholder at a time, so each gets a word of its own.
    for (marker, list) in [("{v}", VERBS), ("{n}", NOUNS), ("{a}", ADJECTIVES)] {
        while let Some(at) = text.find(marker) {
            text.replace_range(at..at + marker.len(), choose(list));
        }
    }
    text
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

    #[test]
    fn listed_names_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for (name, description) in NAMES {
            assert!(seen.insert(*name), "{name} is listed twice");
            assert!(!description.is_empty(), "{name}");
        }
    }

    #[test]
    fn the_docs_list_every_name() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/variables.md");
        let Ok(docs) = std::fs::read_to_string(path) else {
            return;
        };
        for (name, _) in NAMES {
            assert!(
                docs.contains(&format!("{{{{{name}}}}}")),
                "docs/variables.md does not list {name}"
            );
        }
    }

    #[test]
    fn values_have_the_shape_their_name_promises() {
        for _ in 0..50 {
            let version = generate("randomSemver").unwrap();
            assert_eq!(version.split('.').count(), 3, "{version}");
            let latitude: f64 = generate("randomLatitude").unwrap().parse().unwrap();
            assert!((-90.0..=90.0).contains(&latitude));
            let longitude: f64 = generate("randomLongitude").unwrap().parse().unwrap();
            assert!((-180.0..=180.0).contains(&longitude));
            let email = generate("randomExampleEmail").unwrap();
            assert!(email.contains("@example."), "{email}");
            assert_eq!(generate("randomBankAccount").unwrap().len(), 8);
            let phrase = generate("randomPhrase").unwrap();
            assert!(!phrase.contains(['{', '}']), "{phrase}");
            let slug = generate("randomLoremSlug").unwrap();
            assert_eq!(slug.split('-').count(), 3, "{slug}");
            let agent = generate("randomUserAgent").unwrap();
            assert!(agent.starts_with("Mozilla/5.0 ("), "{agent}");
            let recent = generate("randomDateRecent").unwrap();
            assert!(recent.ends_with('Z'), "{recent}");
            let bitcoin = generate("randomBitcoin").unwrap();
            assert!(
                bitcoin.starts_with(['1', '3']) && bitcoin.len() >= 26,
                "{bitcoin}"
            );
        }
        let paragraphs = generate("randomLoremParagraphs").unwrap();
        assert_eq!(paragraphs.lines().count(), 3);
    }

    #[test]
    fn file_values_agree_with_each_other() {
        for _ in 0..50 {
            let (extension, mime, kind) = file_type(false);
            assert!(mime.starts_with(&format!("{kind}/")), "{extension} {mime}");
            let (extension, _, _) = file_type(true);
            assert!(COMMON_EXTENSIONS.split(',').any(|e| e == extension));
            let name = generate("randomCommonFileName").unwrap();
            assert!(name.contains('.'), "{name}");
        }
    }

    #[test]
    fn ibans_pass_the_mod_97_check() {
        for _ in 0..50 {
            let iban = generate("randomBankAccountIban").unwrap();
            let rearranged = format!("{}{}", &iban[4..], &iban[..4]);
            let mut remainder = 0_u32;
            for c in rearranged.chars() {
                let value = c.to_digit(36).unwrap();
                remainder = if value > 9 {
                    (remainder * 100 + value) % 97
                } else {
                    (remainder * 10 + value) % 97
                };
            }
            assert_eq!(remainder, 1, "{iban}");
        }
    }
}
