# Upstream backports

Upstream repository: <https://github.com/ludovic111/horizon>, branch `main`.

| Pointer | Upstream commit |
| --- | --- |
| Most recent backported source | [`eb2b96b6b2551be8494f0ef34189175c4c0e1685`](https://github.com/ludovic111/horizon/commit/eb2b96b6b2551be8494f0ef34189175c4c0e1685) — Draw framed maps without formatting or copying per map per frame |
| Main merge that introduced that source | [`f7002313847f2999705487ba1f5c360029874ad0`](https://github.com/ludovic111/horizon/commit/f7002313847f2999705487ba1f5c360029874ad0) — Merge pull request #7498 from horizon-rust/perf/map-frame-names |
| Upstream tip assessed on 2026-11-03 | [`fc02103818c212920fd24aa34a2b2f91ba49e56f`](https://github.com/ludovic111/horizon/commit/fc02103818c212920fd24aa34a2b2f91ba49e56f) |

These are selective imports. The backport pointer does **not** mean every earlier upstream change
is present, and the assessed tip does **not** mean its code has been imported. “Most recent” means
the recorded source whose first containing merge is latest on upstream `main`'s first-parent history,
not the source's author timestamp or the order in which this fork applied patches. The source SHA
identifies the actual backport; the containing merge records its place on upstream `main`.

## Imported sources

This list records the selective backports made for the browser fork; the fork's original base already
contains other upstream history. An adapted backport retains its source SHA here even when its fork
commit has different content. Each row is present in this branch.

| Upstream source | Change | Import |
| --- | --- | --- |
| [`4e3e038c804d877c53e403b2ed46a9d18dac2f07`](https://github.com/ludovic111/horizon/commit/4e3e038c804d877c53e403b2ed46a9d18dac2f07) | Indexed baked-model lookup | [Fork #6](https://github.com/Yhozen/horizon/pull/6) |
| [`87b3c9aa38e5568733a2678d0332ad33e0e792e3`](https://github.com/ludovic111/horizon/commit/87b3c9aa38e5568733a2678d0332ad33e0e792e3) | Clear stale remote crafting items | Fork #6 |
| [`40c77c02f0332188ae7622dd7b2ae842df5becb7`](https://github.com/ludovic111/horizon/commit/40c77c02f0332188ae7622dd7b2ae842df5becb7) | Faster block-palette access | Fork #6 |
| [`200b756b4ae2cbd0cce186d1b4db8041805c9ac1`](https://github.com/ludovic111/horizon/commit/200b756b4ae2cbd0cce186d1b4db8041805c9ac1) | Skip idle sculk work | Fork #6 |
| [`cd7b6e025e49f19ea8085e8d7734079962e919fe`](https://github.com/ludovic111/horizon/commit/cd7b6e025e49f19ea8085e8d7734079962e919fe) | Reset remote recipe overlays | 2026-10-27 |
| [`87e3df5331fe0825545cdb9d141935382ce3ef10`](https://github.com/ludovic111/horizon/commit/87e3df5331fe0825545cdb9d141935382ce3ef10) | Reset a container's stuck press/drag state | 2026-10-27 |
| [`78bfe7446512fcbf12820b47d168f66d6a4a847d`](https://github.com/ludovic111/horizon/commit/78bfe7446512fcbf12820b47d168f66d6a4a847d) | Integrate the stuck-drag regression test | 2026-10-27 |
| [`f4b4f944e3e4a635d564c000e1c773d4a3d542dc`](https://github.com/ludovic111/horizon/commit/f4b4f944e3e4a635d564c000e1c773d4a3d542dc) | Update the pointer-capture fixture | 2026-10-27 |
| [`1bbe64265c3ae1f45d86cd204acb9b1bbb553a1e`](https://github.com/ludovic111/horizon/commit/1bbe64265c3ae1f45d86cd204acb9b1bbb553a1e) | Save Nether/End chunks with their dimension height | 2026-10-27 |
| [`fb9b26a06cf72d8d9f61fb260d9db76937623713`](https://github.com/ludovic111/horizon/commit/fb9b26a06cf72d8d9f61fb260d9db76937623713) | Cache creative-tab/search stacks | 2026-10-27 |
| [`515c0bdb7d4361d702eb583b6c8f627991705834`](https://github.com/ludovic111/horizon/commit/515c0bdb7d4361d702eb583b6c8f627991705834) | Compute item-model level state once per frame | 2026-10-27 |
| [`0f6569070d13ba42d43db11711d62c572bf2b50d`](https://github.com/ludovic111/horizon/commit/0f6569070d13ba42d43db11711d62c572bf2b50d) | Cache static GUI item vertices and borrow default components | 2026-10-27 |
| [`5389f72b9a7479d791f66977a25b6bfd1bc5d9cb`](https://github.com/ludovic111/horizon/commit/5389f72b9a7479d791f66977a25b6bfd1bc5d9cb) | Invalidate creative cache after resource/language reload | 2026-10-27 |
| [`13e946a89c329411dab3ba3f1256cc34a384a14d`](https://github.com/ludovic111/horizon/commit/13e946a89c329411dab3ba3f1256cc34a384a14d) | Borrow hotbar stacks while drawing | 2026-10-27 |
| [`9223b401e7bb8864711e1f05c891b9d3187bbadb`](https://github.com/ludovic111/horizon/commit/9223b401e7bb8864711e1f05c891b9d3187bbadb) | Decode all item data components (with fork recursion bounds) | [Fork #7](https://github.com/Yhozen/horizon/pull/7) |
| [`67b6e75c0b4361c9aa2fb04ea020410cf0fe5167`](https://github.com/ludovic111/horizon/commit/67b6e75c0b4361c9aa2fb04ea020410cf0fe5167) | The container screen's press state does not survive the screen | 2026-11-02 |
| [`27cd7982a33ff5e51feceff133431cd83cb880f9`](https://github.com/ludovic111/horizon/commit/27cd7982a33ff5e51feceff133431cd83cb880f9) | Fix GPU timer readback after buffer destruction | 2026-11-02 |
| [`7b40465f85ba3b91e8de8ef904e1faf617cf723b`](https://github.com/ludovic111/horizon/commit/7b40465f85ba3b91e8de8ef904e1faf617cf723b) | Fix creative inventory off-hand swap crash on Key F | 2026-11-02 |
| [`501e543105c09ca441cb4c189f86f94138ab4d43`](https://github.com/ludovic111/horizon/commit/501e543105c09ca441cb4c189f86f94138ab4d43) | Creative on a server: the off hand's edits reach the server too | 2026-11-02 |
| [`dab13669f513ec00b24b591188bf3382d2bad96b`](https://github.com/ludovic111/horizon/commit/dab13669f513ec00b24b591188bf3382d2bad96b) | Creative screen drops reach the server: SetCreativeModeSlot(-1) | 2026-11-02 |
| [`bb6305da25ef21321e64faf359b6777aae4000a5`](https://github.com/ludovic111/horizon/commit/bb6305da25ef21321e64faf359b6777aae4000a5) | Keep creative display drops on a server connection | 2026-11-02 |
| [`dbb03a42d3dda5157c030087b1e999ca8fa4a760`](https://github.com/ludovic111/horizon/commit/dbb03a42d3dda5157c030087b1e999ca8fa4a760) | Creative inventory tab: click locally on a server, as vanilla does (#3307) | 2026-11-02 |
| [`f83be65d79d3e1717ba3b99c05ecf77c621545ee`](https://github.com/ludovic111/horizon/commit/f83be65d79d3e1717ba3b99c05ecf77c621545ee) | Send creative inventory edits with the click, not at tick end | 2026-11-02 |
| [`8f2677105330b06ad8e0711ca712294d436d8161`](https://github.com/ludovic111/horizon/commit/8f2677105330b06ad8e0711ca712294d436d8161) | Fall back to level.dat_old and keep an unreadable level.dat | 2026-11-02 |
| [`8ba30576d14f174976a26303cd17e0749c7ad223`](https://github.com/ludovic111/horizon/commit/8ba30576d14f174976a26303cd17e0749c7ad223) | Back up an unreadable player file and fall back to its .dat_old | 2026-11-02 |
| [`c9bfcdcef7a2b9c2de6670d5b1585561eb34e376`](https://github.com/ludovic111/horizon/commit/c9bfcdcef7a2b9c2de6670d5b1585561eb34e376) | Creative item tabs: Q on a hotbar slot drops the item instead of deleting it | 2026-11-02 |
| [`85ec4c8c65ad63b0b39cf55e1843177a69e00a5d`](https://github.com/ludovic111/horizon/commit/85ec4c8c65ad63b0b39cf55e1843177a69e00a5d) | lan_e2e creative_throw: clear each step's drops before the next | 2026-11-02 |
| [`5a6d49212647999d98a054b6eef3ab633d980879`](https://github.com/ludovic111/horizon/commit/5a6d49212647999d98a054b6eef3ab633d980879) | Creative Survival Inventory tab: Q while carrying drops from the slot | 2026-11-02 |
| [`79adacb982f4ac3aa7a5c4bccf1704c30bb0a1e3`](https://github.com/ludovic111/horizon/commit/79adacb982f4ac3aa7a5c4bccf1704c30bb0a1e3) | Cache particle light by exact block within each draw | 2026-11-03 |
| [`eb2b96b6b2551be8494f0ef34189175c4c0e1685`](https://github.com/ludovic111/horizon/commit/eb2b96b6b2551be8494f0ef34189175c4c0e1685) | Reuse map sprite names and borrow framed-map decorations | 2026-11-03 |
| [`11eecf5a6a90e4e412da9da4cf966d26e5cecd0c`](https://github.com/ludovic111/horizon/commit/11eecf5a6a90e4e412da9da4cf966d26e5cecd0c) | Reject ZIP traversal and invalid resource-pack overlays | 2026-11-03 |
| [`04bc30606d09fa618f5e9fef8aec99839b895639`](https://github.com/ludovic111/horizon/commit/04bc30606d09fa618f5e9fef8aec99839b895639) | Keep resource metadata with its owning pack or a higher pack | 2026-11-03 |
| [`11b36e38671f8f3fd22704b586c9402f1bd1fa23`](https://github.com/ludovic111/horizon/commit/11b36e38671f8f3fd22704b586c9402f1bd1fa23) | Use separate saved-NBT limits without relaxing network bounds | 2026-11-03 |
| [`9f0d819f6ba6392fcde3b6ec730be663a2114936`](https://github.com/ludovic111/horizon/commit/9f0d819f6ba6392fcde3b6ec730be663a2114936) | Persist sanitized region headers on chunk write and clear | 2026-11-03 |
| [`be35f0cd18fcce995c7a97706b757f02615fda0f`](https://github.com/ludovic111/horizon/commit/be35f0cd18fcce995c7a97706b757f02615fda0f) | Never infer imported-world ownership from another player file | 2026-11-03 |
| [`be7de6230c1610a8790968f449a9ecf6a1abd7b4`](https://github.com/ludovic111/horizon/commit/be7de6230c1610a8790968f449a9ecf6a1abd7b4) | Load the named world owner before the joining profile | 2026-11-03 |
| [`7ca3d989013018000e7f7f99c14635b3ba7cafcd`](https://github.com/ludovic111/horizon/commit/7ca3d989013018000e7f7f99c14635b3ba7cafcd) | Save the owner file that will be reloaded and recover guest backups | 2026-11-03 |
| [`152617ece70af3670b46b2345e4683fd4defaf65`](https://github.com/ludovic111/horizon/commit/152617ece70af3670b46b2345e4683fd4defaf65) | Log player save failures (adapted player-save portion) | 2026-11-03 |

## Latest comparison

The 2026-11-03 comparison used merged fork main `5af117e792bb65b79d6cd48f640d89f006a6a536` and
examined browser-relevant rendering, inventory/input and save/load changes among the 836 non-merge
commits since the previous assessed tip, `90d7eb16355904d4b78258bdb071d37ec5a2fbd9`. Older deferred
fixes were revisited too. The source table records the selective fixes now implemented on this branch. The backport pointer
advances by upstream integration order, independently of the order of our imports. The merged code retains the PR #10 model-identity and
respawn-cache corrections, and PR #18's real display-click/carried-stack regression setup.

The save imports are adapted to preserve readable backups through a subsequent save, validate
level/player NBT roots, retain legacy generator/version checks, and keep recoverable worlds visible
in the world picker. They cover the single-player owner in both execution modes and LAN guest
player-file recovery, including preserving readable backups during the next save.

Recommended next imports, all **not imported**:

| Area | Sources | Scope and dependencies |
| --- | --- | --- |
| Normal-container offhand actions | `b70cb22e9d094319f71c541be9a6ca7a9e5c9647`, `d9f93b474f36138c590eb518f36fe4f9a4a4a5b9`, `34e889990df1910043044c1507c01d0a1d50a58a`; tests `8987ae75eb3e29aa02ecf876832757dac8d20f72`, `c686e9eeef95dc2d0f8d8d987bd211a7d725b519` | Chest F swaps and shield/armor shift-click routing on the server; about 225 lines with tests. Separate from the imported creative fixes. |
| Disconnect input reset | `698e7fd662ba0a6502f0965ea660994f4f11092f` | Clear held input before reconnecting; extend coverage to this fork's physical modifier keys. |

Imported-world owner identity now uses the named owner file or the joining profile's own file,
with validated backup recovery. Both in-frame and integrated loading avoid selecting another
player's file by modification time; player saves preserve corrupt originals and readable backups.

The new station panic fix `b04bd9a0e33b61509d50509c0ce5064d57442ba8` does not apply: the offending
upstream result-slot snapshot is absent here. Server block/entity picking, bulk teleport/occlusion
removal, worker cancellation on exit, native staging buffers and worker-based GUI atlas compression
remain separate work requiring their dependencies and browser evaluation.

The initial comparison checked source/history, patch applicability and prepared R2 item definitions.
Implementation verification is recorded in the backport PRs. Runtime checks use isolated temporary
worlds and the prepared R2 resources; vanilla-oracle execution additionally requires the game JARs.

## Maintaining the record

1. Fetch upstream without replacing this fork's branch:

   ```sh
   git fetch https://github.com/ludovic111/horizon.git main:refs/remotes/upstream/main
   ```

2. Inspect changes after the **assessed tip**, and revisit older deferred fixes as needed. Do not use
   the backport pointer as a claim that everything before it is covered. Update the assessed tip and
   date when another comparison is completed, even if no change is imported.
3. After importing and checking a change, add its full original source SHA to the table in the same
   commit as the backport. Keep that SHA in the fork commit's message too, including for adaptations.
4. Advance the backport pointer only when the imported source entered upstream `main` later than the
   current pointer. Find the first main first-parent commit containing the source with
   `git merge-base --is-ancestor <source> <main-commit>`; its preceding first-parent commit must not
   contain the source. If several imported sources entered in the same merge, choose the descendant
   source containing the preceding imported fixes, or retain the existing pointer if they are unrelated.
5. Remove a row if its change is reverted and recompute the pointer when necessary. Move pending PRs
   into the table only when their code is included in the branch carrying this record.
