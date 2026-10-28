# Running in CI

`getman run` sends every request of a workspace, or of one folder, in
order, runs their scripts and checks, and reports what happened. It reads
the same files you edit in the UI, so the requests you try by hand are the
tests CI runs. Nothing is exported, uploaded or kept in sync.

```sh
getman run                     # the whole workspace
getman run orders              # one folder
getman run orders/create.yaml  # one request
getman run login.yaml orders   # several, in the order given
getman run -e staging          # with an environment
```

```text
Orders API with the staging environment: 3 requests

  ✓ POST    Create order   201 Created 182 ms · 96 B
      ✓ status == 201
      ✓ json.id exists
  ✗ GET     Get order      404 Not Found 41 ms · 23 B
      ✗ status == 200
        status is 404; expected == 200
  ✓ DELETE  Archive order  204 No Content 37 ms · 0 B

  Requests  3 run · 1 failed · 0 skipped
  Checks    2 passed · 1 failed
  Time      0.31 s total · 87 ms average response
```

Requests run in the order the sidebar shows them: folders first unless a
folder's `order` says otherwise. Several targets run one after another,
in the order you give them; a request named twice runs twice. A request **fails** when it cannot be
sent, when one of its scripts throws, or when a test or check fails. A
`500` with no checks is not a failure, so give the requests that matter
a `status` check.

WebSocket, event stream and Socket.IO requests are connected to. A run
sends the request's saved messages, collects what arrives until the
request's `until` (10 messages or 5 seconds when it has none), then runs
the post-response script, checks and captures, with the handshake as the
response and the messages received as `gm.messages` and the `messages`
check target. Their results count and are reported like any other request's.
See [WebSocket, SSE and Socket.IO](realtime.md#scripts-and-checks).

## Options

| Option | Does |
| --- | --- |
| `-e`, `--env NAME` | Use an environment, by file name or display name. `GETMAN_ENVIRONMENT` sets it too |
| `--var NAME=VALUE` | Set a variable above every other scope. Repeat for more |
| `-d`, `--data FILE` | Run once per row of a CSV or JSON file |
| `-n`, `--iterations N` | Run N times. Defaults to the number of data rows, or 1 |
| `--delay MS` | Wait between requests |
| `--bail` | Stop at the first failure |
| `-r`, `--reporter NAME[=PATH]` | `cli`, `json`, `junit`, `html`, `csv` or `xlsx`; repeat for several, `=PATH` writes to a file (`xlsx` needs one) |
| `-v`, `--verbose` | Print every response's status line, headers and body, and script console output |
| `--script-timeout SECONDS` | How long a script may run. Default 10 |
| `--persist` | Save variables the run set to the git-ignored local files, as the UI does |
| `--http-version VERSION` | `auto`, `http1`, `http2` or `http3` for every request, over the files' settings. See [HTTP and connections](http.md) |
| `--proxy URL` | A proxy URL, `system` or `off` for every request, over the files' settings |
| `--save` | Keep the run in `.getman/runs`, where the UI's Runner lists it |
| `-w`, `--workspace DIR` | The workspace, when it is not the current directory or a parent |

## Exit status

| Status | Means |
| --- | --- |
| 0 | Everything passed |
| 1 | A request failed, or the run was stopped by `--bail` |
| 2 | The run could not start: no workspace, an unknown environment, a bad data file |

## Data files

With `--data`, each row's columns are variables for one iteration,
stronger than the environment. Scripts read them through
`gm.iterationData`.

```csv
email,plan
ada@example.com,pro
grace@example.com,free
```

```json
[
  { "email": "ada@example.com", "plan": "pro" },
  { "email": "grace@example.com", "plan": "free", "seats": 3 }
]
```

CSV needs a header row. JSON must be an array of objects; values that are
not strings are passed as JSON text.

## Reports

| Reporter | Writes |
| --- | --- |
| `cli` | The run in the terminal, as it happens. The default |
| `json` | The whole run: a summary, and for each request its URL, status, time and where it went (DNS, connect, first byte, download), HTTP version, size, tests, checks, script errors, missing variables and console output |
| `junit` | JUnit XML, with one test suite per request and one test case per test or check, which most CI systems can show |
| `html` | One page for people: the totals, then every request with its status, timings and checks. Failed requests are open, the rest fold away |
| `csv` | One row per request per iteration: name, path, method, URL, status, times, size, result, test and check counts and the reasons it failed |
| `xlsx` | An Excel workbook with a Summary sheet and a Requests sheet of the same rows as the CSV |

```sh
getman run -e staging -r cli -r junit=getman.xml -r html=getman.html
```

The HTML report is a single file with its styles inside. It loads
nothing, runs no script, and prints cleanly to PDF from a browser, so you
can attach it to a CI run or mail it. In the CSV, a cell that starts
with `=`, `+`, `-` or `@` gets a leading `'`, so a spreadsheet does not
run it as a formula.

Reports leave out bodies and headers, so tokens in them stay out of CI
artifacts. URLs are kept: put secrets in headers, not in query strings.

## In the browser

Open the Runner from a folder's menu, the folder's page, or the command
palette. It lists every request the run covers. Untick the ones to leave
out, and drag a request, or focus its handle and press the arrow keys, to
change the order. The files are not changed. The Runner shows the
`getman run` command that does the same run from a terminal.

Every run started there is kept in the workspace's git-ignored
`.getman/runs`, the newest 20, and so is a `getman run --save`. The
Runner lists them under **Past runs**: select one to see its results
again, and use **Report** to download any of them as HTML, Excel, CSV,
JSON or JUnit XML. Kept runs leave out bodies, headers and variable
values.

## Secrets

Pass secrets as environment variables named `GETMAN_*`, and refer to them
as `{{$env.GETMAN_NAME}}`. Only that prefix can be read, so a workspace
cannot pick up other credentials the CI job has. See
[Variables](variables.md#secrets-from-the-environment).

## GitHub Actions

```yaml
name: API checks
on: [push, pull_request]

jobs:
  api:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v5
      - name: Install Getman
        run: |
          curl -fsSL https://fzn0x.github.io/getman/install.sh | sh
          echo "$HOME/.local/bin" >> "$GITHUB_PATH"
      - name: Run the API checks
        working-directory: api
        run: getman run -e staging -r cli -r junit=getman.xml
        env:
          GETMAN_API_TOKEN: ${{ secrets.API_TOKEN }}
      - uses: actions/upload-artifact@v5
        if: always()
        with:
          name: getman-report
          path: api/getman.xml
```

To test a service the job starts itself, start it in an earlier step and
point an environment at it, or pass `--var baseUrl=http://127.0.0.1:8080`.

## GitLab CI

```yaml
api-checks:
  image: debian:stable-slim
  before_script:
    - apt-get update && apt-get install -y curl ca-certificates
    - curl -fsSL https://fzn0x.github.io/getman/install.sh | sh
    - export PATH="$HOME/.local/bin:$PATH"
  script:
    - cd api
    - getman run -e staging -r cli -r junit=getman.xml
  artifacts:
    when: always
    reports:
      junit: api/getman.xml
```

## One request at a time

`getman send` sends a single request and prints the response body, so it
fits in shell pipelines:

```sh
getman send users/me.yaml -e staging | jq .email
getman send orders/create.yaml --var sku=BOOK-7 -i   # with status and headers
getman send health.yaml --json                       # everything, as JSON
```

It exits with 1 when the request fails its checks or tests, like
`getman run`.
