# Import and export

## From Postman

Export a collection from Postman as **Collection v2.1** (v2.0 works too),
then import it in the UI (<kbd>Alt</kbd> <kbd>I</kbd>, then pick or drop
the file) or from the terminal:

```sh
getman import "Orders API.postman_collection.json"
```

The collection becomes a folder of the workspace. Its folders become
directories and its requests YAML files, in the same order. The
collection's variables, authentication and scripts move to that folder's
`_folder.yaml`, so they apply to the same requests as before.

```sh
getman import orders.json --folder legacy   # inside an existing folder
getman import orders.json --root            # as the workspace itself
```

With `--root`, the collection's settings go into `getman.yaml` and its
requests to the top of the workspace, which suits a new repository.

**Environments and globals** import the same way:

```sh
getman import staging.postman_environment.json   # environments/staging.yaml
getman import workspace.postman_globals.json     # into getman.local.yaml
```

Variables Postman marks as secret are listed under `secrets`, and their
values go to the git-ignored `staging.local.yaml`, never to the file you
commit.

### What carries over

| | |
| --- | --- |
| Requests | Method, URL, query and path parameters (disabled ones kept), headers, description |
| Bodies | Raw (JSON, XML, text), form, multipart, binary file, GraphQL |
| Authentication | Basic, Bearer, API key, Digest, OAuth 1.0, OAuth 2.0 (every grant), AWS Signature, JWT, Hawk, Akamai EdgeGrid, ASAP |
| Scripts | Pre-request and test scripts, unchanged: `pm` is the same object as `gm` |
| Variables | Collection, folder, environment and global variables |
| Settings | Redirects, TLS verification |

### What is reported instead

Getman prints a warning for each thing it could not bring over, rather
than dropping it quietly:

- **Saved example responses.** Getman does not keep examples.
- **Files** selected for form fields or binary bodies. Exports hold the
  absolute path on the exporting machine; Getman keeps the file name and
  asks you to copy the file into the workspace.
- **Saved OAuth 2.0 tokens.** They would end up in a shared file; get a
  new one under **Auth**. A callback URL Getman cannot listen on, such as
  Postman's own, is dropped too: Getman receives the sign-in on this
  machine ([Authentication](auth.md#signing-in-with-a-browser)).
- **Other authentication types**, such as NTLM. Set the credentials in
  headers or a pre-request script.

## From cURL

Paste a cURL command into the URL bar and Getman turns it into a request.
Or use the Import dialog, or the terminal:

```sh
getman import --curl 'curl -X POST https://api.example.com/orders -H "Content-Type: application/json" -d "{\"sku\":\"BOOK-42\"}"'
pbpaste | getman import -        # macOS clipboard
```

Commands copied from browser developer tools work, in both their bash and
Windows `cmd` forms. Getman understands the method, headers, data (as
JSON, form or raw text), `--data-urlencode`, `-F` form fields and files,
`-u` with `--digest` or `--basic`, `-G`, `-I`, `-A`, `-e`, `-b`, `-L`,
`--max-redirs`, `-k`, `-m` and `--oauth2-bearer`. Options that make no
sense for a saved request, such as `-o` or `--proxy`, are skipped.

## Export as code

<kbd>Alt</kbd> <kbd>C</kbd> in the UI, or `getman export`, writes the
current request as code, with variables replaced and authentication
applied. Signatures, such as AWS Signature or OAuth 1.0, are made as of
now and soon expire; OAuth 2.0 tokens and Digest answers need a round
trip, so they are left out:

```sh
getman export orders/create.yaml --to python -e staging
```

| `--to` | Writes |
| --- | --- |
| `curl` | A cURL command (the default) |
| `httpie` | An HTTPie command |
| `powershell` | `Invoke-WebRequest` |
| `fetch` | JavaScript `fetch` |
| `python` | Python with `requests` |
| `go` | Go with `net/http` |
| `rust` | Rust with `reqwest` |
| `java` | Java with `java.net.http` |

Exported code contains the real values of your variables, tokens included.
Check it before you paste it somewhere public.

## Out of Getman

There is nothing to export. A workspace is a directory of YAML files that
any tool can read, diff and generate, and that you keep however you keep
code.
