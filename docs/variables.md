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

`{{$name}}` makes a new value each time it appears:

| Variable | Value |
| --- | --- |
| `{{$uuid}}`, `{{$guid}}`, `{{$randomUUID}}` | A random UUID v4 |
| `{{$timestamp}}` | Unix time in seconds |
| `{{$timestampMs}}` | Unix time in milliseconds |
| `{{$isoTimestamp}}` | The current time in ISO 8601, UTC |
| `{{$randomInt}}` | A whole number from 0 to 1000 |
| `{{$randomBoolean}}` | `true` or `false` |
| `{{$randomAlphaNumeric}}`, `{{$randomHexadecimal}}` | One character |
| `{{$randomFirstName}}`, `{{$randomLastName}}`, `{{$randomFullName}}` | Names |
| `{{$randomUserName}}`, `{{$randomEmail}}`, `{{$randomPassword}}` | Account details; emails use example.com |
| `{{$randomPhoneNumber}}` | A number like 555-0142-3381 |
| `{{$randomCity}}`, `{{$randomCountry}}`, `{{$randomCountryCode}}` | Places |
| `{{$randomColor}}`, `{{$randomHexColor}}` | Colours |
| `{{$randomWord}}`, `{{$randomWords}}`, `{{$randomLoremSentence}}` | Words |
| `{{$randomIP}}`, `{{$randomIPV6}}`, `{{$randomMACAddress}}` | Addresses |
| `{{$randomDomainName}}`, `{{$randomUrl}}` | Under `example` and `example.com` |
| `{{$randomPrice}}` | A price like 42.17 |
| `{{$randomDatePast}}`, `{{$randomDateFuture}}` | A date within a year, ISO 8601 |

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
