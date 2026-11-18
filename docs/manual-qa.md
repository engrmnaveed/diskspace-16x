# Manual macOS QA

Test on the oldest supported macOS version and a current version. Record the macOS version, CPU, filesystem, app build, and result for each run.

## Make a test folder

```sh
fixture="$(mktemp -d)"
mkdir -p "$fixture/Photos/Shoots" "$fixture/Archives"
dd if=/dev/urandom of="$fixture/Photos/Shoots/contact-sheet.bin" bs=1048576 count=8
dd if=/dev/urandom of="$fixture/Archives/small.bin" bs=1048576 count=1
ln "$fixture/Photos/Shoots/contact-sheet.bin" "$fixture/Photos/Shoots/linked-copy.bin"
ln -s "$fixture/Photos" "$fixture/shortcut"
mkfile -n 64m "$fixture/Archives/sparse.bin"
open "$fixture"
```

## Run the checks

- Open the fixture with the picker and then drop the folder from Finder. Repeat for a readable mounted local volume.
- Drill down through **Photos** and **Shoots**. Find the largest allocated file with the keyboard, reveal it in Finder, then repeat with the pointer. Copy its path and verify it matches the revealed item.
- Switch to **Largest Files** and change the metric. With equal filesystem metadata, confirm the allocated ranking shows the contact sheet and the logical ranking puts the sparse file first. Compare the totals and paths with CLI output for both metrics.
- Switch between the list and **Folder Treemap**. Check that the largest visible area follows the ranking metric. Activate a folder tile to drill in and a file tile to select its path.
- Navigate by breadcrumb, keyboard, and the app menus. Confirm the selected row, progress, issues, and actionable controls are legible with VoiceOver and in both appearances.
- Confirm the link alias and symbolic link never add to the bytes. Compare root totals with the CLI using the same folder, metric, and snapshot in time.
- Add an unreadable descendant as a standard user. Confirm the app keeps readable items, qualifies the totals, and identifies the denied path. Restore permissions after the check. Confirm an unreadable root fails with a clear message.
- Start scanning a large synthetic tree and immediately cancel. Confirm the window stays responsive and a subsequent scan can finish. Close the window during a scan and verify the operation stops.
- Verify scans do not modify or move the fixture items.

Clean up the temporary folder when finished.
