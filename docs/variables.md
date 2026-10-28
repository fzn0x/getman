# Variables and environments

Write `{{name}}` anywhere in a request: the URL, headers, parameters,
bodies, authentication fields. Getman replaces it just before sending.

```yaml
url: "{{baseUrl}}/users/{{userId}}"
```

A variable's value can contain other variables; they are replaced too. A
variable that is not defined is sent as written, `{{name}}`, and the
response view lists it so a typo does not go unnoticed. In the editors,
known variables are purple and unknown ones are underlined in red.

## Where values come from

When two places define the same name, the more specific wins. From
weakest to strongest:

1. **Globals**: in `getman.local.yaml`, on this machine only
2. **Workspace variables**: in `getman.yaml`
3. **The selected environment**: `environments/<name>.yaml`, then its
   `.local.yaml`
4. **Folder variables**: in each `_folder.yaml`, outermost first
5. **Request variables**: the request's own `variables`
6. **The current data row**, in a run with a data file
7. **Local variables**: set by scripts with `gm.variables.set`, for the
   rest of the run

The UI shows the scope a value comes from when you complete a variable
name after typing `{{`.

## Dynamic values

`{{$name}}` makes a new value each time it appears. Every name from
Postman's dynamic variables works, so imported requests keep working.
Scripts can make the same values with `gm.variables.replaceIn("{{$randomEmail}}")`.

| Variable | Value |
| --- | --- |
| `{{$uuid}}`, `{{$guid}}`, `{{$randomUUID}}` | A random UUID v4 |
| `{{$timestamp}}`, `{{$timestampMs}}` | Unix time in seconds, in milliseconds |
| `{{$isoTimestamp}}` | The current time in ISO 8601, UTC |
| `{{$randomInt}}` | A whole number from 0 to 1000 |
| `{{$randomBoolean}}` | `true` or `false` |
| `{{$randomAlphaNumeric}}`, `{{$randomHexadecimal}}` | One character |
| `{{$randomPassword}}` | 16 letters and digits |
| `{{$randomSemver}}` | A version such as `3.14.2` |
| `{{$randomLocale}}`, `{{$randomAbbreviation}}` | A locale code such as `pt-BR`; an abbreviation such as `TCP` |

### People, places and companies

| Variable | Value |
| --- | --- |
| `{{$randomFirstName}}`, `{{$randomLastName}}`, `{{$randomFullName}}` | Names |
| `{{$randomNamePrefix}}`, `{{$randomNameSuffix}}` | `Dr`, `Jr` and the like |
| `{{$randomUserName}}`, `{{$randomEmail}}`, `{{$randomExampleEmail}}` | Account details; emails use example.com, or example.net and example.org for `$randomExampleEmail` |
| `{{$randomPhoneNumber}}`, `{{$randomPhoneNumberExt}}` | `555-0142-3381`; `94-315-425-6711` |
| `{{$randomCity}}`, `{{$randomCountry}}`, `{{$randomCountryCode}}` | Places |
| `{{$randomStreetName}}`, `{{$randomStreetAddress}}` | `Okafor Street`; `412 Okafor Street` |
| `{{$randomLatitude}}`, `{{$randomLongitude}}` | Coordinates with four decimals |
| `{{$randomJobArea}}`, `{{$randomJobDescriptor}}`, `{{$randomJobType}}`, `{{$randomJobTitle}}` | Job titles and their parts |
| `{{$randomCompanyName}}`, `{{$randomCompanySuffix}}` | A company name; `LLC`, `Group` |
| `{{$randomBs}}`, `{{$randomBsAdjective}}`, `{{$randomBsBuzz}}`, `{{$randomBsNoun}}` | Business jargon |
| `{{$randomCatchPhrase}}`, `{{$randomCatchPhraseAdjective}}`, `{{$randomCatchPhraseDescriptor}}`, `{{$randomCatchPhraseNoun}}` | Slogans |

### Internet

| Variable | Value |
| --- | --- |
| `{{$randomIP}}`, `{{$randomIPV6}}`, `{{$randomMACAddress}}` | Addresses |
| `{{$randomDomainName}}`, `{{$randomDomainSuffix}}`, `{{$randomDomainWord}}`, `{{$randomUrl}}` | Under `example` and `example.com` |
| `{{$randomProtocol}}` | `http` or `https` |
| `{{$randomUserAgent}}` | A browser `User-Agent` string |
| `{{$randomColor}}`, `{{$randomHexColor}}` | Colours |
| `{{$randomAvatarImage}}`, `{{$randomImageUrl}}` | A URL under example.com |
| `{{$randomAbstractImage}}`, `{{$randomAnimalsImage}}`, `{{$randomBusinessImage}}`, `{{$randomCatsImage}}`, `{{$randomCityImage}}`, `{{$randomFoodImage}}`, `{{$randomNightlifeImage}}`, `{{$randomFashionImage}}`, `{{$randomPeopleImage}}`, `{{$randomNatureImage}}`, `{{$randomSportsImage}}`, `{{$randomTechnicsImage}}`, `{{$randomTransportImage}}` | An image URL of that kind under example.com |
| `{{$randomImageDataUri}}` | A small SVG as a `data:` URI |

### Dates

| Variable | Value |
| --- | --- |
| `{{$randomDatePast}}`, `{{$randomDateFuture}}` | A date within a year, ISO 8601 |
| `{{$randomDateRecent}}` | A date within the last day, ISO 8601 |
| `{{$randomWeekday}}`, `{{$randomMonth}}` | `Tuesday`, `March` |

### Money

| Variable | Value |
| --- | --- |
| `{{$randomPrice}}` | A price like 42.17 |
| `{{$randomCurrencyCode}}`, `{{$randomCurrencyName}}`, `{{$randomCurrencySymbol}}` | `EUR`, `Euro`, `€` (each is picked on its own) |
| `{{$randomBankAccount}}`, `{{$randomBankAccountName}}` | 8 digits; `Savings Account` |
| `{{$randomBankAccountBic}}`, `{{$randomBankAccountIban}}` | A BIC; an IBAN whose check digits are right |
| `{{$randomCreditCardMask}}` | Four digits |
| `{{$randomTransactionType}}` | `deposit`, `payment` and the like |
| `{{$randomBitcoin}}` | A string shaped like a Bitcoin address; its checksum is not real |

### Products and data

| Variable | Value |
| --- | --- |
| `{{$randomDepartment}}`, `{{$randomProduct}}`, `{{$randomProductAdjective}}`, `{{$randomProductMaterial}}`, `{{$randomProductName}}` | Shop items such as `Sleek Oak Kettle` |
| `{{$randomDatabaseColumn}}`, `{{$randomDatabaseType}}`, `{{$randomDatabaseCollation}}`, `{{$randomDatabaseEngine}}` | Database names |
| `{{$randomFileName}}`, `{{$randomFileExt}}`, `{{$randomFileType}}`, `{{$randomMimeType}}` | `lantern.pdf`, `pdf`, `application`, `application/pdf` |
| `{{$randomCommonFileName}}`, `{{$randomCommonFileExt}}`, `{{$randomCommonFileType}}` | The same, limited to everyday extensions |
| `{{$randomFilePath}}`, `{{$randomDirectoryPath}}` | Paths such as `/var/log/lantern.txt` |

### Words and text

| Variable | Value |
| --- | --- |
| `{{$randomWord}}`, `{{$randomWords}}` | A word; two to four words |
| `{{$randomNoun}}`, `{{$randomVerb}}`, `{{$randomIngverb}}`, `{{$randomAdjective}}` | One word of that kind |
| `{{$randomPhrase}}` | A short sentence |
| `{{$randomLoremWord}}`, `{{$randomLoremWords}}` | One word; three words |
| `{{$randomLoremSlug}}` | Three words joined by dashes |
| `{{$randomLoremSentence}}`, `{{$randomLoremSentences}}` | One sentence; two to five |
| `{{$randomLoremLines}}` | One to four sentences, one per line |
| `{{$randomLoremParagraph}}`, `{{$randomLoremParagraphs}}` | One paragraph; three, one per line |
| `{{$randomLoremText}}` | Placeholder text of a random length |

## Secrets from the environment

`{{$env.GETMAN_NAME}}` reads an environment variable of the process.
Only names that start with `GETMAN_` can be read: a collection someone
sent you cannot read your other secrets, such as cloud credentials, just
by referring to them.

This is the simplest way to give CI a secret:

```yaml
# GitHub Actions
- run: getman run --env staging
  env:
    GETMAN_API_TOKEN: ${{ secrets.API_TOKEN }}
```

```yaml
# in a request or environment file
auth:
  type: bearer
  token: "{{$env.GETMAN_API_TOKEN}}"
```

`getman run --var name=value` and `getman send --var name=value` set a
variable for one run, above every other scope.

## Values that stay on your machine

Three files are never committed (`getman init` adds them to `.gitignore`):

| File | Holds |
| --- | --- |
| `environments/<name>.local.yaml` | Local values of an environment: secrets, tokens, your own base URL |
| `getman.local.yaml` | Local overrides of workspace variables, and globals |
| `.getman/` | History of sent requests and their responses, the trash, your header presets |

When a script sets a variable with `gm.environment.set`,
`gm.collectionVariables.set` or `gm.globals.set` while you work in the UI,
the value is saved to the matching local file. `getman run` keeps such
values in memory for the run only, unless you pass `--persist`.
