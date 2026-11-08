# Real-window E2E (planned)

This directory is the contract for the layered E2E work described in
`docs/TESTING.md`. It is intentionally not a green suite yet.

Planned pieces:

1. **Test driver**, feature-gated behind `berth-app/test-driver`:
   newline-delimited JSON over a `0600` Unix socket, token from
   `BERTH_TEST_TOKEN`, commands `hello/status/tab/select/search/act/refresh/quit`.
   Never compiled into a release artifact.
2. **xa11y** (`pip install xa11y pytest-xa11y`) locators:
   `button[name="Containers"]`, `text_field[name="Search containers"]`,
   `row[name="berth-demo-web"]`, `dialog[name="Delete berth-demo-web?"]`.
3. **Smoke flow**: launch with a temp data dir → first snapshot → switch to
   Images → search → open inspector → close → switch to Disk → prune sheet
   cancel → quit. Assert the window stays responsive and no schema request
   overlaps.
4. **Live flow**: disposable engine, create/pause/resume/stop/delete a container,
   assert before/after with the engine CLI.

Until this lands, `docs/PARITY-CHECKLIST.md` lists the gap explicitly.
