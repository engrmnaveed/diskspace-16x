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
- Add an unreadable descendant (`chmod 0`) as a standard user. Confirm the app keeps readable items and shows the folder as **Protected** with a lock. The notice should be neutral rather than an orange warning, and the sidebar should read "Protected items skipped". Scan Details should list the folder under Protected items. In the CLI, confirm `[protected]`, a one-line stderr summary, and exit code 2. Restore permissions after the check. Confirm an unreadable root fails with a clear message.
- Without Full Disk Access, scan `/` or your home folder. Confirm the neutral notice offers **Allow Full Disk Access…** and that it opens the Full Disk Access pane. Scan Details should list protected items grouped by containing folder, after any scan errors. Grant access to the build under test and return to the app. Confirm the notice offers **Scan Again**, and that the rescan no longer lists `~/Library/Containers`. Remove the grant afterwards if the test machine needs it off.
- Scan a large synthetic tree with files at several depths. Confirm a browseable two-level preview appears before deeper results. Drill into a pending folder and check that it fills in without losing the current location. Cancel from the progress strip and confirm the incomplete totals remain labeled. Close the window during another scan and verify the operation stops.
- Scan a test folder with local items and cloud-only entries. Compare placeholder availability before and after. Confirm cloud-only rows have zero allocated bytes and the contents of cloud-only directories are left unexpanded. If a known provider directory stalls, confirm other local branches finish and the skipped path appears in Scan Details.
- Click the **Name**, **Allocated**, and **Items** column headers in the folder list and in **Largest Files**. Confirm the sort indicator moves, a second click reverses the order, and the selected row stays selected. Check that **Photos** shows 3 items, counting the hard link once, and that the treemap and donut still rank by size.
- Choose **View → Size → Apparent Size**. Confirm the column header, the size summary, the shares, and the charts switch, and the sparse file now ranks first at 64 MB. Quit and reopen the app and confirm the choice persists. Compare with `space16x scan "$fixture" --apparent --depth 2`.
- In **Settings → Exclude from scans**, enter `Archives` and `*.bin` on separate lines, then `/` on a third line. Confirm the warning appears for `/` and a scan shows the error notice. Remove `/`, scan again, and confirm the header shows the excluded count, **Archives** is missing, and Scan Details explains the exclusion. Compare with `space16x scan "$fixture" --exclude Archives --exclude '*.bin'`. Clear the patterns afterwards.
- Verify scans do not modify or move the fixture items.
- Make a disposable copy of the fixture and scan it. Select two rows with ⌘-click, press ⌘⌫, and confirm the alert names the item count and allocated size. Click **Cancel** and confirm nothing moved. Press ⌘⌫ again, click **Move to Trash**, and confirm the rows disappear while the folder total, the parent's item count, and the charts drop by the same amount without a rescan. Check that the notice says space is freed when you empty the Trash. Open the Trash in Finder and confirm both items are there. Note whether Finder offers **Put Back** for them. Rescan and compare totals with `space16x scan`.
- Try **Move to Trash…** from a row's context menu, from the selection bar, and on a cloud-only item and the protected `chmod 0` folder. Confirm the last two stay put with a notice that explains why. Confirm the command is disabled while a scan runs.
- In another disposable copy, run `space16x` in Terminal. Mark a file and a folder with Space, press `d`, and confirm the footer names the count and size. Press `n`, then `d` and `y`. Confirm both rows leave and the totals drop. In Finder's Trash, choose **Put Back** for each item and confirm it returns to its folder. Press `r` in the explorer and confirm the items reappear with their sizes. Run `space16x --read-only` and confirm Space and `d` only show the read-only notice.
- Select a file and press Space. Confirm Quick Look opens, the arrow keys move the selection and the preview follows, and Space closes it. Try **Browse → Quick Look** (⌘Y) and the context menu. Select a folder and confirm Quick Look shows its summary. Select a cloud-only item and confirm Space shows a notice instead of downloading it.
- Open a folder of the fixture, add a large file to it in Terminal, and choose **Scan → Rescan Folder** (⌥⌘R). Confirm the progress strip names the folder, the new file appears, and the folder, the parent totals, the item counts, and the charts grow without a full rescan. The notice gives the new and earlier sizes. Delete the folder in Terminal and choose **Rescan Folder** from its row's context menu, then confirm it leaves the results. Start a rescan of a large folder and cancel it, and confirm the earlier results stay. Compare totals with `space16x scan`.

Clean up the temporary folder when finished.
