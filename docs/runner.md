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
getman Orders API with the staging environment: 3 requests

  ✓ POST    Create order   201 Created 182 ms · 96 B
      ✓ status == 201
      ✓ json.id exists
  ✗ GET     Get order      404 Not Found 41 ms · 23 B
      ✗ status == 200
        status is 404; expected == 200
  ✓ DELETE  Archive order  204 No Content 37 ms · 0 B

  Requests  3 run · 1 failed · 0 skipped
  Tests     0 passed · 0 failed
  Checks    2 passed · 1 failed
  Time      310 ms total · 87 ms average response

  Failed.
```

Under each request the terminal lists its script tests and checks, the
reason it could not be sent, script errors, missing variables and
warnings, such as a capture that found nothing. A skipped request is
marked `–`. With several iterations, each starts under an `Iteration
2/3` heading. The last line is `All passed.` or `Failed.`, after the
reason when the run stopped early. A run a script stopped on purpose still
ends with `All passed.` when nothing failed.

Requests run in the order the sidebar shows them: folders first unless a
folder's `order` says otherwise. Several targets run one after another,
in the order you give them; a request named twice runs twice. A request **fails** when its file
cannot be read, when it cannot be sent, when one of its scripts throws,
or when a test or check fails. A
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
| `TARGET`... | Folders and requests to run, in this order. Default: the whole workspace |
| `-e`, `--env NAME` | Use an environment, by file name or display name. `GETMAN_ENVIRONMENT` sets it too |
| `--var NAME=VALUE` | Set a variable above every other scope. Repeat for more |
| `-d`, `--data FILE` | Run once per row of a CSV or JSON file |
| `-n`, `--iterations N` | Run N times. Defaults to the number of data rows, or 1 |
| `--delay MS` | Milliseconds to wait between the requests of an iteration. Default 0 |
| `--bail` | Stop at the first failure |
| `-r`, `--reporter NAME[=PATH]` | `cli`, `json`, `junit`, `html`, `csv` or `xlsx`, in any case; repeat for several, `=PATH` writes to a file (`xlsx` needs one). Default `cli` |
| `-v`, `--verbose` | Print every response's status line, headers and body, and script console output |
| `--script-timeout SECONDS` | How long a script may run. Default 10 |
| `--no-scripts` | Run no pre-request or post-response scripts; checks and captures still run. See [Scripts](scripts.md#turning-scripts-off) |
| `--persist` | Save variables the run set to the git-ignored local files, as the UI does |
| `--http-version VERSION` | `auto`, `http1`, `http2` or `http3` for every request, over the files' settings. See [HTTP and connections](http.md) |
| `--proxy URL` | A proxy URL, `system` or `off` for every request, over the files' settings |
| `--save` | Keep the run in `.getman/runs`, where the UI's Runner lists it |
| `-w`, `--workspace DIR` | The workspace, when it is not the current directory or a parent |

With `--persist`, values that scripts and captures set for the
environment go to its `.local.yaml`, and workspace and global values to
`getman.local.yaml`. Session and local variables are not saved. The
values are saved whether or not the run passed.

A reporter without `=PATH` writes to standard output after the run, and
`cli` writes there as the run goes, so to pipe a report, as in `-r json |
jq .summary`, leave `cli` out. `=PATH` creates the folders the path
needs, and the run notes each file it wrote on standard error.

## Exit status

| Status | Means |
| --- | --- |
| 0 | Everything passed. A run that a script ended on purpose with `gm.execution.stopExecution()` counts, when nothing failed |
| 1 | A request, test or check failed, or the run was stopped early by `--bail` or by <kbd>Ctrl</kbd> <kbd>C</kbd> |
| 2 | The run could not start, or its results could not be written: no workspace, an unknown target or environment, a bad data file, a report or `--persist` file that cannot be written |

The Runner in the UI follows the same rule: a run a script ended on
purpose is marked as passed when nothing failed.

## Run control

Each iteration runs the planned requests in order. Variables that
scripts and captures set in one iteration are still set in the next;
only the data row changes. Between two requests of an iteration, the
run waits `--delay` milliseconds.

Scripts steer the run with `gm.execution`; see
[Controlling a run](scripts.md#controlling-a-run):

| Call | In a run |
| --- | --- |
| `gm.execution.setNextRequest(name)` | After this request, go on with the request of that name or path in the run, ahead or back. A name that is not in the run ends the iteration with a warning |
| `gm.execution.setNextRequest(null)` | End this iteration after this request; the next iteration starts |
| `gm.execution.skipRequest()` | In a pre-request script: do not send this request. It counts as skipped, not run |
| `gm.execution.stopExecution()` | Finish this request and end the run. The run exits with 0 when nothing failed, and with 1 when something did, such as a request before it |

An iteration that runs more than 100 requests for each one planned, plus
1,000, is stopped with a warning, since `setNextRequest` probably loops.
Warnings do not change the exit status.

`--bail` ends the run after the first request that fails, and
<kbd>Ctrl</kbd> <kbd>C</kbd> ends it after the request that is running.
Either way the reports are written, and the terminal, JSON, HTML and
Excel reports give the reason the run stopped.

## Data files

With `--data`, each row's columns are variables for one iteration,
stronger than the environment, folder and request variables and weaker
than session and local ones; see
[Variables](variables.md#where-values-come-from). Scripts read them
through `gm.iterationData`.

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

A file whose name ends in `.json` is read as JSON, and any other file as
CSV. CSV needs a header row, and every row needs as many fields as the
header; names in the header are trimmed, values are kept as they are,
and fields can be quoted, as in `"Lovelace, A"`. JSON must be an array
of objects; values that are not strings are passed as JSON text.

The run has as many iterations as the file has rows, unless `-n` says
otherwise. With more iterations than rows, the last row repeats; with
fewer, the rows after the last iteration are not used.

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

Reports leave out bodies and headers, so tokens in them stay out of CI
artifacts. The values of the variables the environment lists under
`secrets` show as `[secret]` wherever a run shows them: in the terminal,
in every report, in kept runs and in the browser's Runner, also inside a
URL, a test's name or a script's console output. So do the values
`--var`, a data file, a script or a capture gives those variables during
the run. Their JSON-escaped and percent-encoded forms are hidden too,
and for values of four bytes or more their hex and Base64 forms.

### JSON

The report holds the Getman version, the number of iterations, the
summary, and one entry per request per iteration, in the order they ran.
Iterations count from 0 here.

```json
{
  "getman": "0.3.4",
  "iterations": 1,
  "summary": {
    "iterations": 1, "requests": 3, "failed_requests": 1,
    "skipped_requests": 0, "tests_passed": 0, "tests_failed": 0,
    "checks_passed": 2, "checks_failed": 1,
    "duration_ms": 310.4, "average_response_ms": 86.67,
    "stopped": null, "stopped_by_script": false, "warnings": []
  },
  "requests": [
    {
      "iteration": 0, "name": "Get order", "path": "orders/get-order.yaml",
      "method": "GET", "url": "https://staging.example.com/orders/7",
      "status": 404, "time_ms": 41.2,
      "timings": { "dns_ms": 0.4, "connect_ms": 12.1, "first_byte_ms": 38.0, "download_ms": 0.3, "reused": false },
      "http_version": "HTTP/1.1", "size": 23, "passed": false, "skipped": false, "error": null,
      "script_errors": [], "tests": [],
      "checks": [{ "name": "status == 200", "passed": false, "message": "status is 404; expected == 200" }],
      "missing_variables": [], "warnings": [], "logs": []
    }
  ]
}
```

`summary.stopped` is the reason the run ended early, and
`summary.stopped_by_script` is `true` when a script ended it on purpose
with `stopExecution()`. Such a run passes when nothing failed. The
`summary.warnings` are the problems that did not stop it. A request that was
never sent has `null` for its response fields and the reason in `error`.

### JUnit

The `<testsuites>` element is named after the workspace. Each request of
each iteration is a `<testsuite>`, named after the request, with
`(iteration 2)` after the name when there are several. Its test cases
have the request's path as their class name:

- one per script test and per check; a failed one holds a `<failure>`
  with the message, and a skipped test is `<skipped/>`;
- `request`, holding an `<error>`, when the request could not be sent;
- `script in ...`, holding an `<error>`, for each script that threw;
- `request` alone, passing or skipped, when the request has nothing else
  to report.

A suite's time is the response time in seconds.

### HTML

The page opens with the workspace, what ran, the environment, the start
time in UTC and, when there are several, the number of iterations, then
the verdict and the
totals of requests, tests, checks and time. Each request shows its
method, name, status and time, with a bar comparing its time to the
slowest; open it for its URL, timings, the messages of a connection,
errors, checks, script tests, missing variables and warnings. Failed
requests start open. The page follows the system's light or dark theme.

The HTML report is a single file with its styles inside. It loads
nothing, runs no script, and prints cleanly to PDF from a browser, so you
can attach it to a CI run or mail it.

### CSV and Excel

The CSV has these columns, one row per request per iteration, with
iterations counted from 1:

```text
iteration,name,path,method,url,status,status_text,time_ms,first_byte_ms,size_bytes,result,tests_passed,tests_failed,checks_passed,checks_failed,failures
```

`result` is `passed`, `failed` or `skipped`, and `failures` lists the
reasons, separated by ` | `. Times are milliseconds and sizes are body
bytes. A cell that starts with `=`, `+`, `-`, `@`, a tab or a carriage
return gets a leading `'`, so a spreadsheet does not run it as a
formula.

The Excel workbook's **Summary** sheet has the workspace, what ran, the
environment, the start time in UTC, the result, the reason it stopped,
any warnings, the counts of iterations, requests, tests and checks, and
the total and average times. Its **Requests** sheet has the CSV's rows,
with the header row frozen and a filter on every column. Text over
32,000 bytes is cut, since Excel refuses longer cells.

## In the browser

Open the Runner from a folder's menu, the folder's page, or the command
palette. It lists every request the run covers. Untick the ones to leave
out, and drag a request, or focus its handle and press the arrow keys, to
change the order. The files are not changed. The Runner shows the
`getman run` command that does the same run from a terminal.

Every run started there is kept in the workspace's git-ignored
`.getman/runs`, the newest 20, and so is a `getman run --save`. The
Runner lists them under **Past runs**: select one to see its results
again, use **Report** to download any of them as HTML, Excel, CSV,
JSON or JUnit XML, or **Forget** to delete one. A downloaded report is
named after the workspace and the start time, as in
`getman-orders-2026-10-14-0507.html`. Kept runs leave out bodies,
headers and variable values.

## Secrets

Pass secrets as environment variables named `GETMAN_*`, and refer to them
as `{{$env.GETMAN_NAME}}`. Only that prefix can be read, so a workspace
cannot pick up other credentials the CI job has. See
[Variables](variables.md#secrets-from-the-environment).

A value read this way is hidden in what the run shows only when it is the
value of a variable the environment lists under `secrets`. Used straight
in a request, it is not known to be secret, so send it in a header, which
reports leave out, rather than in the URL.

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
