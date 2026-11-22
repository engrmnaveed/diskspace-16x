# HTML report structure

`expected.json` describes the page that `space16x scan --html` and the Mac app's **Export HTML Report…** both write for the scan in `fixtures/cleanup/scan.json`: the heading, the sections in order, the first rows of the Contents, Largest files and File types tables, and text the page must never contain. Sizes are left out because each app formats them its own way. The Rust test `html::tests::matches_the_shared_structure` and the Swift test `HTMLReportTests` read it.
Each app names files without an extension its own way, so the file types compared are those with one.
