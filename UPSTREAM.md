# Upstream backports

Upstream repository: <https://github.com/ludovic111/horizon>, branch `main`.

| Pointer | Upstream commit |
| --- | --- |
| Most recent backported source | [`27cd7982a33ff5e51feceff133431cd83cb880f9`](https://github.com/ludovic111/horizon/commit/27cd7982a33ff5e51feceff133431cd83cb880f9) — Fix GPU timer readback after buffer destruction |
| Main merge that introduced that source | [`0f365f9404aaae2f13b6af5d15cc50548bd4feb9`](https://github.com/ludovic111/horizon/commit/0f365f9404aaae2f13b6af5d15cc50548bd4feb9) — upstream PR #6354 |
| Upstream tip assessed on 2026-10-27 | [`51447edda45e15bfa6b91ae57e78689767a4cf72`](https://github.com/ludovic111/horizon/commit/51447edda45e15bfa6b91ae57e78689767a4cf72) |

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
