# Changelog

All notable changes to `appstore-mcp`. This project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **App Asset Library** (App Store Connect API 4.5.1), 12 tools in the
  `assets` group: `get_app_asset_library`, `list_asset_library_images`,
  `list_asset_library_videos`, `upload_asset_library_image`,
  `upload_asset_library_video`, `delete_asset_library_image`,
  `delete_asset_library_video`, `create_asset_library_placement`,
  `list_asset_library_placements`, `delete_asset_library_placement`,
  `set_asset_library_placement_order`, and `list_asset_library_ref_data`.
  Uploads reuse the existing reserve → upload → commit flow; the library's
  commit sends `{ uploaded: true }` without `sourceFileChecksum`, which its
  update request does not define.

### Changed

- The screenshot-set, preview-set, and in-app event screenshot tools now say in
  their descriptions that Apple deprecated those resources in API 4.5.1 and
  name the App Asset Library tool to use instead. They are not removed.

## [0.4.0] — 2026-09-23

### Added

- Optional `ASC_TOOL_DISCOVERY=1` mode exposes only `search_tools`,
  `get_tool_details`, and `call_discovered_tool`. Search and execution use the
  same `ASC_TOOLS` and `ASC_READ_ONLY` filters as the normal tool surface.
  The generic call is conservatively annotated as destructive when writes are
  enabled; inspect the target tool's annotations before approving writes.
- Repository contributor guidance in `AGENTS.md`.

### Security

- Updated locked `h2` and `rustls` versions to include fixes for
  RUSTSEC-2026-0258 and RUSTSEC-2026-0285.

## [0.3.1] — 2026-08-06

The two minor items left over from 0.3.0's profiling. Internal only — no tool,
parameter, or output change.

### Changed

- **The release binary is 25.8% smaller**: 8.15 MB → 6.05 MB, via whole-program
  LTO in a single codegen unit. It adds ~42 s to a full release build, which
  only runs in the release workflow and in parallel across the three platforms,
  while the binary ships three times over inside every `.mcpb`. Debug and test
  builds are unaffected.

  `panic = "abort"` would shrink it further and was deliberately not used: a
  panic in one tool handler would take the whole server down rather than failing
  that single call, and an MCP client on stdio would lose its session.

- **Deduplicating `included` resources across pages is no longer quadratic.**
  `merge_page` rebuilt its seen-set from every resource accumulated so far on
  each page, so a 20-page walk re-hashed the same resources nineteen times over.
  The set is now built once in `get_paged` and carried through. Identities are
  also one allocation instead of two. Worst case (20 pages × 200 sideloaded
  resources): **6.00 ms → 2.76 ms**.

### Added

- Tests that the seen-set survives across all pages rather than just the last,
  that `get_paged` deduplicates sideloaded resources over HTTP, and that a
  resource identity cannot confuse `("ab", "c")` with `("a", "bc")`.

## [0.3.0] — 2026-08-06

Performance and resource use, driven by measurement rather than guesswork.
Profiling first ruled out the usual suspects: startup is ~5 ms, idle RSS is
7.5 MB, and generating 114 tool schemas doesn't register. The costs were
elsewhere.

### Changed

- **Tool results are serialized compactly.** Indented JSON measured **1.72×**
  the bytes of the same document — 72% more tokens for identical information,
  and a byte budget that carried ~40% less of the data actually asked for. A
  large `list_apps` page now delivers **421 items** within the 60 KB budget.
  This is visible in output: results are no longer pretty-printed.
- **Analytics segments stream.** The gzip decoder now feeds the CSV reader
  directly, and the download is handed on as `Bytes` instead of being copied
  into a `Vec`. A 1.2 MB segment expanding to 10.6 MB previously held **11.8 MB**
  of intermediates; peak is now the compressed buffer alone. Parsing 300k rows
  also got faster: **28 ms → 14.7 ms**.
- **Report parsing runs on a blocking thread.** Gunzipping and parsing is
  synchronous CPU work — ~15 ms for 300k rows and linear beyond that — and was
  stalling an async worker for the duration.
- **Response capping stopped rebuilding the document to measure it.** Sizes are
  now counted through a writer that allocates nothing, and the kept prefix of
  `data` is found in a single pass using the fact that compact JSON costs
  exactly one comma between items — replacing a binary search that cloned and
  re-serialized a candidate document on every probe. Rendering a 2000-item page:
  **8.59 ms → 4.00 ms**, against a 2.15 ms floor for serializing it once, and
  the multi-megabyte throwaway allocations are gone.
- **Upload chunks are reference-counted.** `Bytes` instead of `Vec<u8>` means
  handing a chunk to a retry attempt no longer copies it — previously even the
  first, successful attempt duplicated the whole chunk.

### Added

- `tests/packaging_metadata.rs` asserts what previously only failed at release
  time: the `server.json` description within the MCP Registry's 100-character
  limit, the crate version agreeing across all four packaging files, the
  release-asset URL, and a `CHANGELOG` entry for the current version.
- A test that the kept prefix is the *largest* one that fits, not merely one
  that fits, and a test that measured length always equals serialized length —
  the size arithmetic rests on that.

### Fixed

- The `server.json` description exceeded the MCP Registry's 100-character limit,
  which failed the v0.2.0 registry publish with a 422. Released artifacts were
  unaffected.

## [0.2.0] — 2026-08-06

Hardening and context control. No breaking changes to existing tool calls: every
tool keeps its name and parameters, and every new knob is opt-in or defaults to
the previous behaviour.

### Added

- **`download_analytics_segment`** (tool 114) closes the analytics loop. The
  other analytics tools navigate to a presigned segment URL; this one fetches it,
  gunzips it, and returns the rows as JSON keyed by column name, with a
  `max_rows` sample and the true row count. Handles both tab- and
  comma-separated segments, and quoted fields.
- **Tool annotations.** Every tool now advertises `readOnlyHint`,
  `destructiveHint`, `idempotentHint`, and `openWorldHint`, so a client can tell
  `list_apps` from `remove_user` instead of treating every call alike. Effects
  are derived from each tool's leading verb, and a tool whose verb isn't
  recognised fails the test suite rather than shipping unlabelled.
- **`ASC_READ_ONLY`** serves only the 35 tools that cannot modify the account.
  `appstore_request` is kept but refuses any method other than `GET`, so the
  escape hatch can still reach uncovered endpoints without being a way around
  the restriction.
- **`ASC_TOOLS`** serves only the groups you name (or the `core` preset, 41
  tools), cutting the per-session context cost of tool definitions. An
  unrecognised group serves nothing and says so, rather than quietly falling
  back to everything.
- **`max_pages` on `appstore_list`** follows `links.next` and merges up to 20
  pages into one result, turning an N-turn cursor loop into one call.
  `meta.pagesFetched` and `meta.hasMore` report what happened.
- **Request timeouts** (`ASC_TIMEOUT_SECS`, `ASC_CONNECT_TIMEOUT_SECS`,
  `ASC_TRANSFER_TIMEOUT_SECS`). There were none before, so a stalled connection
  hung the tool call indefinitely with no way to cancel it over stdio.
- **Retries with backoff** (`ASC_MAX_RETRIES`, default 3). A `429` is replayed
  for any method since Apple rejected it without applying it; a `5xx` or
  mid-flight timeout is replayed only for idempotent methods — never a `POST`,
  which could otherwise create a duplicate resource. Honours `Retry-After`,
  bounded so a tool call can't stall for minutes.
- **Response shaping** (`ASC_COMPACT_RESPONSES`, `ASC_MAX_RESPONSE_BYTES`).
  Per-resource `self` links and link-only relationships are stripped — often
  most of a JSON:API page — and an oversized response sheds `included` and then
  trailing `data` items, carrying a `_truncated` key that says what went missing.
- Declared MSRV (1.88), verified by its own CI job, plus an advisory
  `cargo audit` job.

### Changed

- **Asset uploads stream from disk.** The file used to be read into memory whole
  and each chunk copied again; now the checksum is computed block by block and
  each operation reads only its own byte range, so peak memory is one chunk
  rather than the size of the asset. A chunk that fails is retried on its own
  instead of discarding the whole upload.
- **`appstore_request` validates the HTTP method** against the documented set.
  A typo like `FETCH` is a valid HTTP token, so it used to become a real request
  and come back as an opaque error.
- The crate is now a library plus a thin binary, so tests can drive a real server
  against a mock API.
- CI asserts the invariants of the tool surface — unique names, real
  descriptions, object schemas, correct annotations, and that the filtering knobs
  withhold exactly what they claim — instead of a hardcoded tool count that broke
  on every addition while proving little.

### Testing

96 → 202 tests. The new ones cover the HTTP layer that unit tests on body
builders can't reach: retry and no-retry decisions, timeouts, pagination and
merging, the three-step upload protocol (byte ranges, checksum, absent
`Authorization` header on presigned `PUT`s), segment download and parsing, and
the served tool surface under each configuration.

## [0.1.0]

Initial release: 113 tools across the App Store Connect API, plus two generic
JSON:API escape-hatch tools.
