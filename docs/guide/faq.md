# FAQ and troubleshooting

## Is DiskUse16x safe to use?

Yes. DiskUse16x only reads the names, sizes and dates of your files while it scans. It never opens them, changes them or sends anything anywhere. It works offline and has no account.

The one change it ever makes to your files is moving the items you pick to the Trash, after you confirm. It never deletes anything for good, and it never copies your files.

Besides that, it writes only files you ask for: a saved scan or report, and in Pro, a scan you save to the history or a Footprint receipt. Pro's duplicate finder is the only feature that reads what's inside files, and only after you start it.

## macOS says it can't verify the app

DiskUse16x is free and isn't notarized by Apple, so macOS asks you to approve it once. Click **Done** in the warning, not **Move to Trash**. Then open **System Settings → Privacy & Security**, scroll to **Security**, and click **Open Anyway**. [Allow the first launch](../install.md#allow-the-first-launch) has every step, including what to do if **Open Anyway** isn't there.

## Some folders say "Protected"

macOS keeps some folders private to the apps that own them, such as Mail, Safari and other apps' data. DiskUse16x lists them as **Protected** and doesn't count what's inside. That's expected, not an error.

To include them on a Mac, give DiskUse16x **Full Disk Access**:

1. Click **Allow Full Disk Access…** in DiskUse16x's notice, or open **System Settings → Privacy & Security → Full Disk Access**.
2. Turn on **DiskUse16x**. If it isn't listed, click **+** and choose it from Applications.
3. Back in DiskUse16x, click **Scan Again**. If folders are still protected, quit and reopen the app.

For the terminal, give Full Disk Access to the terminal app you use, such as Terminal.

A few system folders stay protected even then, and on Linux, other users' folders need administrator access. DiskUse16x still lists them, so you know what the totals leave out. If protected folders come back after an update, turn **DiskUse16x** off and on again in Full Disk Access. [Full Disk Access](../install.md#full-disk-access) has more.

## Why don't the sizes match Finder?

A few honest reasons, from most to least common:

- **Space on disk versus file length.** DiskUse16x counts the space each file takes on the disk, called its **allocated size**. Finder's Get Info shows that as "on disk", after the file's length. They differ for compressed files, very small files and files with empty regions. To count lengths instead, choose **View → Size → Apparent Size** in the app, or press `a` in the terminal.
- **Copies that share space.** On a Mac, a copy made in Finder can share its space with the original, so both look full-size but use the space once. DiskUse16x counts each file's full size.
- **Hard links.** One file can appear in two places. DiskUse16x counts it once, at the first place it finds it.
- **Space no folder shows.** Your disk also holds snapshots, purgeable space, swap and other system volumes. That's why a scan of your whole disk rarely matches the free space Finder reports. Pro's [Explain Disk Space](pro.md#explain-disk-space) shows this space.
- **Units.** DiskUse16x uses the same units as Finder: 1 GB is 1,000,000,000 bytes. Some other tools use binary units, where 1 GiB is 1,073,741,824 bytes, which makes their numbers about 7% smaller.

## I moved files to the Trash, but my free space didn't change

Empty the Trash. Space comes back only then. If it still doesn't, the files may have shared their space with clones, hard links or a recent Time Machine snapshot. See [Get the space back](clean-up.md#get-the-space-back).

## What does "Partial scan" mean?

Something inside the folder couldn't be read, such as a file that disappeared during the scan or a folder you don't have permission to open. DiskUse16x keeps going and shows everything else. The totals count what it could read.

To see what was skipped, click **Scan Details** in the app's sidebar (⌘I), or press `e` in the terminal. In scripts, `diskuse16x scan` exits with code 2 for a partial scan.

## Why does an iCloud folder show almost nothing?

Files that live only in iCloud, or in another cloud service, aren't on your disk. DiskUse16x shows them as **Cloud only** and counts them as zero, and it never downloads them.

## The scan is slow

- Scans of external hard disks and network drives are slower than your internal disk. In the terminal, add `--threads 1` to read one item at a time, which suits those disks better.
- Skip big folders you don't need, such as `node_modules`, with [exclude patterns](find-what-uses-space.md#skip-folders-you-dont-care-about).
- You don't have to wait. The first two levels appear quickly, and you can browse while the rest fills in.

## Is Pro really free?

Yes. Basic and Pro are both free and open source. Pro adds views that use more memory or do more work, so Basic stays light enough for small servers. On the Mac, Pro is a switch in Settings. In the terminal, it's a separate install. See [DiskUse16x Pro](pro.md).


## What happened to Space 16X?

It's the same app with a new name. Space 16X became DiskUse16x in version 1.0, and the command `space16x` became `diskuse16x`, or `du16x` for short. Updating moves you over and keeps your settings and saved scans. On a Mac, turn on Full Disk Access for **DiskUse16x** again if you use it. [Upgrading from Space 16X](../install.md#upgrading-from-space-16x) has the details.
## How do I update DiskUse16x?

DiskUse16x never checks for updates by itself. In the Mac app, choose **Help → Check for Updates…** to open the newest release's page in your browser.

- **Mac app:** if you installed with Homebrew, run `brew upgrade --cask diskuse16x`. Otherwise, download the new ZIP and replace the app in Applications. macOS may ask you to approve the first launch again.
- **Terminal:** run the one-line installer again. If you installed with Homebrew, run `brew upgrade du16x`.

[Update DiskUse16x](../install.md#update-diskuse16x) has every way.

## How do I uninstall DiskUse16x?

- **Mac app:** if you installed with Homebrew, run `brew uninstall --cask diskuse16x`. Otherwise, move **DiskUse16x** from Applications to the Trash.
- **Terminal:** run the installer with `--uninstall`, as [Uninstall the managed CLI](../install.md#uninstall-the-managed-cli) shows. If you installed with Homebrew, run `brew uninstall du16x`. For a Linux package, run `sudo apt remove diskuse16x` or `sudo dnf remove diskuse16x`. Add `-pro` to the name for Pro.

## Something's wrong, or I have an idea

Open an issue on [GitHub](https://github.com/engrmnaveed/diskuse16x/issues). Say what you did, what you expected and what happened, plus your system version. Please remove personal names and paths from screenshots and reports.
