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
- Switch to **Largest Files**. Confirm the contact sheet comes first and the sparse file shows its actual allocated size. Compare the totals and paths with the CLI's allocated report.
- Switch between **List**, **Treemap**, and **Donut**. Check that chart tiles, slices, and shares follow allocated bytes within the current folder. Activate a folder or file from the chart.
- Navigate by breadcrumb, keyboard, and the app menus. Confirm the selected row, progress, issues, and actionable controls are legible with VoiceOver and in both appearances.
- Confirm the link alias and symbolic link never add to the bytes. Compare root totals with the CLI against the same fixture before it changes.
- Compare a few folder sizes with the on-disk figure in Finder's Get Info, which uses allocated bytes. The app and the CLI should show the same decimal values, such as `45.89 GB`. Hover a size in the list, the donut center, and a treemap tile, and confirm the tooltip shows the exact bytes and the GiB value. VoiceOver should read only the exact bytes. In the CLI explorer, open a file's details and confirm they show the same GiB value and byte count.
- Scan `/` on an APFS startup disk. In the app and CLI, confirm `/Users` is present and its contents do not reappear under `/System/Volumes/Data/Users`. Scan `/System/Volumes/Data` directly and confirm it includes its own `Users` folder.
- Add an unreadable descendant as a standard user. Confirm the app keeps readable items, qualifies the totals, and identifies the denied path. Restore permissions after the check. Confirm an unreadable root fails with a clear message.
- Scan a large synthetic tree with files at several depths. Confirm a browseable two-level preview appears before deeper results. Drill into a pending folder and check that it fills in without losing the current location. Cancel from the progress strip and confirm the incomplete totals remain labeled. Close the window during another scan and verify the operation stops.
- Scan a test folder with local items and cloud-only entries. Compare placeholder availability before and after. Confirm cloud-only rows have zero allocated bytes and the contents of cloud-only directories are left unexpanded. If a known provider directory stalls, confirm other local branches finish and the skipped path appears in Scan Details.
- Verify scans do not modify or move the fixture items.

Clean up the temporary folder when finished.
