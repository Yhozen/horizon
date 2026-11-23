# Upstream backports

Upstream repository: <https://github.com/horizon-rust/horizon>, branch `main`.

| Pointer | Upstream commit |
| --- | --- |
| Most recent backported source | [`0a9d9a871562561b1c8a5964326603f942c8c089`](https://github.com/horizon-rust/horizon/commit/0a9d9a871562561b1c8a5964326603f942c8c089) — Reject negative trim pattern holders in received recipes |
| Main merge that introduced that source | [`85b5bcca7c79bb470b90bdb5914b413aa66fe7ae`](https://github.com/horizon-rust/horizon/commit/85b5bcca7c79bb470b90bdb5914b413aa66fe7ae) — Merge pull request #17145 from kacpherek/mod-api-merge |
| Upstream tip assessed on 2026-11-17 (second pass) | [`34fcb5c361e9fba8b951f695f4e288e081d8e031`](https://github.com/horizon-rust/horizon/commit/34fcb5c361e9fba8b951f695f4e288e081d8e031) |

## Licence boundary: imports stopped

This fork stays under the **MIT licence**. Upstream moved to LGPL-3.0-or-later in
[`fb1135c6020b09ba8d4645bd70b0a94a405f8ad1`](https://github.com/horizon-rust/horizon/commit/fb1135c6020b09ba8d4645bd70b0a94a405f8ad1)
(2026-11-15). **No code, tests or comments are taken from upstream after that change.** Only code
already MIT at upstream `fb1135c6020^` (`59dff1a36bb3a2308f120480e57a2e01c4ba1aae`) may still be
imported; check with `git merge-base --is-ancestor <source> 59dff1a36bb3a2308f120480e57a2e01c4ba1aae`.
Later upstream bugs may be noted as reports, and fixed independently from vanilla's behaviour and this
fork's own code, without reading or copying upstream's patch.

One recorded source, `0a9d9a871562561b1c8a5964326603f942c8c089` (a 3-line negative-holder guard), was
authored on 2026-11-10 in a tree still licensed MIT, but entered upstream `main` after the change.
Every other row is an ancestor of `59dff1a36b`.

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
| [`698e7fd662ba0a6502f0965ea660994f4f11092f`](https://github.com/ludovic111/horizon/commit/698e7fd662ba0a6502f0965ea660994f4f11092f) | Release held input on server disconnect, including physical modifiers | 2026-11-03 |
| [`b70cb22e9d094319f71c541be9a6ca7a9e5c9647`](https://github.com/ludovic111/horizon/commit/b70cb22e9d094319f71c541be9a6ca7a9e5c9647) | Allow server offhand swaps in normal containers | 2026-11-03 |
| [`d9f93b474f36138c590eb518f36fe4f9a4a4a5b9`](https://github.com/ludovic111/horizon/commit/d9f93b474f36138c590eb518f36fe4f9a4a4a5b9) | Shift-click equippable items into an empty offhand | 2026-11-03 |
| [`8987ae75eb3e29aa02ecf876832757dac8d20f72`](https://github.com/ludovic111/horizon/commit/8987ae75eb3e29aa02ecf876832757dac8d20f72) | Cover offhand shift-click destinations and occupied fallback | 2026-11-03 |
| [`34e889990df1910043044c1507c01d0a1d50a58a`](https://github.com/ludovic111/horizon/commit/34e889990df1910043044c1507c01d0a1d50a58a) | Shift-click offhand armor into its equipment slot | 2026-11-03 |
| [`c686e9eeef95dc2d0f8d8d987bd211a7d725b519`](https://github.com/ludovic111/horizon/commit/c686e9eeef95dc2d0f8d8d987bd211a7d725b519) | Cover armor-from-offhand and occupied equipment fallback | 2026-11-03 |
| [`e550c9d85c09efc5c52417563deabf5b2fb2cbea`](https://github.com/ludovic111/horizon/commit/e550c9d85c09efc5c52417563deabf5b2fb2cbea) | Reject invalid or repeated server quick-craft starts | 2026-11-03 |
| [`078bdb45f4a0349735ccecd682b4832097db9458`](https://github.com/ludovic111/horizon/commit/078bdb45f4a0349735ccecd682b4832097db9458) | Cover invalid drag types and normal quick-craft behavior | 2026-11-03 |
| [`e6c1a91a6549d6fb3128f95ae5b7c88dba4a4e6d`](https://github.com/ludovic111/horizon/commit/e6c1a91a6549d6fb3128f95ae5b7c88dba4a4e6d) | Reset creative search and scrolling when reopening the inventory | 2026-11-03 |
| [`16a4b6010b1fb948c7bf2a0647303bbba01fb25c`](https://github.com/ludovic111/horizon/commit/16a4b6010b1fb948c7bf2a0647303bbba01fb25c) | Bound SNBT parser nesting before the thread stack overflows | 2026-11-06 |
| [`3f529b7d808c0ac9adfdc49bd533769a878bdf14`](https://github.com/ludovic111/horizon/commit/3f529b7d808c0ac9adfdc49bd533769a878bdf14) | Keep SNBT depth safe for server stacks and valid nested items | 2026-11-06 |
| [`6bf332f5c5246b2412858c416145e0a6271d8cee`](https://github.com/ludovic111/horizon/commit/6bf332f5c5246b2412858c416145e0a6271d8cee) | Reject excessively nested command redirects before stack overflow | 2026-11-06 |
| [`45af2b23b2a0d341563325def664e40be127d51c`](https://github.com/ludovic111/horizon/commit/45af2b23b2a0d341563325def664e40be127d51c) | Bound network NBT tag allocations within a conservative quota | 2026-11-06 |
| [`d55c2840a7884404324ed4bf4e5fdefb4bfa2277`](https://github.com/ludovic111/horizon/commit/d55c2840a7884404324ed4bf4e5fdefb4bfa2277) | Map unknown server block states to air before indexed lookups | 2026-11-06 |
| [`22ca656476539a657758652b67c868e241003e38`](https://github.com/ludovic111/horizon/commit/22ca656476539a657758652b67c868e241003e38) | Bound fetched PNG dimensions before allocating pixels | 2026-11-06 |
| [`a7f6ecbdfbf11c5626c69d3b5c159deac690657f`](https://github.com/ludovic111/horizon/commit/a7f6ecbdfbf11c5626c69d3b5c159deac690657f) | Synchronize the selected hotbar slot before an immediate Q drop | 2026-11-06 |
| [`53b7a6768de77939e2e191cd9ef4f03d4a4a4968`](https://github.com/horizon-rust/horizon/commit/53b7a6768de77939e2e191cd9ef4f03d4a4a4968) | Resync the selected slot after a predicted drop and immediate pickup | 2026-11-17 |
| [`fbfa055effff0800b0223652751268551a1f4344`](https://github.com/horizon-rust/horizon/commit/fbfa055effff0800b0223652751268551a1f4344) | Prevent loot item-count arithmetic from overflowing | 2026-11-17 |
| [`70865d77e8fabcb097a48066d81e1b8f01b48e89`](https://github.com/horizon-rust/horizon/commit/70865d77e8fabcb097a48066d81e1b8f01b48e89) | Bound saved boat values and widen chunk coordinate arithmetic (partial import; newer void lifecycle excluded) | 2026-11-17 |
| [`04449d8b7380875dc594a0c0f60c9354c2d97b4b`](https://github.com/horizon-rust/horizon/commit/04449d8b7380875dc594a0c0f60c9354c2d97b4b) | Prevent server chunk radius and centre arithmetic overflow | 2026-11-17 |
| [`d0da18c651306cc0f12176c0920bd37cc8b5be2c`](https://github.com/horizon-rust/horizon/commit/d0da18c651306cc0f12176c0920bd37cc8b5be2c) | Bound received login RSA keys to Java size limits | 2026-11-17 |
| [`5fe1090401e63235ce743d1b3da45e2520964cb7`](https://github.com/horizon-rust/horizon/commit/5fe1090401e63235ce743d1b3da45e2520964cb7) | Reject negative item-component holder IDs and holder-set counts | 2026-11-17 |
| [`0a9d9a871562561b1c8a5964326603f942c8c089`](https://github.com/horizon-rust/horizon/commit/0a9d9a871562561b1c8a5964326603f942c8c089) | Reject negative trim pattern holders in received recipes | 2026-11-17 |
| [`1f28bbe8d6f69079abe092183c08c6a11aee57cf`](https://github.com/horizon-rust/horizon/commit/1f28bbe8d6f69079abe092183c08c6a11aee57cf) | Handle signed wire offsets without overflow (numeric subset only) | 2026-11-17 |

## Latest comparison

The 2026-11-17 comparison fetched fork main at
`c49254ee46d763842888db8ac97dcef597d59ea0`, unchanged since the previous pass.
PRs #29–#33 are still open; this branch includes that stack and appends selective fixes.
The range after the previous assessed tip `41f822b659789780f58d32c63ddfb6887ea1b8bf`
contains 16,097 non-merge commits and 2,944 first-parent entries, including a large mod-API
branch merge. Screening used commit subjects and subsystem histories, then source diffs,
follow-ups and applicability checks for the important candidates. It is not an exhaustive
line-by-line review of all 16,097 commits.

Each added source row records code present in this branch. Partial imports are marked:
newer mod, launcher, shader and protocol-version features are not prerequisites of these fixes.
The reviewed tip remains separate from the latest backported source.

Excluded after checking fork paths: the biome-table crash (`85b61b6f6e46b1d3217e2b01c0357350969c1e37`)
requires replaceable world biome tables; this fork maps server biomes into an immutable table.
Prepared-instance overflow (`970d95419055a56f192eb0722f9285a999fdad3d`) requires absent native-mod
rendering. Packet component recursion is already bounded by the fork. Crash-save handling
(`b0f0be4e209bc039e10e46d527d616c454435174`, `cbe92651a1cebf2820b48beb00ae5bbfc3eba4c1`)
is deferred: the worker exit path and dedicated recovery/guest-state safeguards differ, so
blindly saving partially unwound state would risk player data. Newer asynchronous chunk-save
backlog APIs are absent. Missing/corrupt region regeneration changes are deferred to a separate
recovery review. Cosmetic changes, optional PvP extras and general refactors are excluded.

All selected source versions still carry MIT; their source commits precede upstream's licence
change `fb1135c6020b09ba8d4645bd70b0a94a405f8ad1`. Prior model-cache identity, respawn-cache,
display-click, owner identity, backup and inventory fixes are preserved. Verification is recorded
in the backport PRs. Runtime checks use isolated homes and prepared R2 resources; Java-oracle
execution additionally requires Minecraft JARs, which those resources do not contain.

### Second 2026-11-17 pass

`810f09f5bc35c33d39534a33269654075d575122..34fcb5c361e9fba8b951f695f4e288e081d8e031`: 19 commits,
mostly tooling, CI and docs. **Licence:** upstream moved from MIT to LGPL-3.0-or-later in
`fb1135c6020b09ba8d4645bd70b0a94a405f8ad1` (2026-11-15); every commit in this range is LGPL, so none is
copied into this MIT fork, and nothing is added to the imported-source table.

- Dead mount inventory duplication (reported upstream by `da7afd64bb99586dc30fb7d8bc795df3308527ac`):
  fixed independently from vanilla's `handleContainerClick`/`stillValid` rule, using this fork's own
  existing range check. No upstream code or test copied.
- Silk-touched hive bees/honey sync (`fd3142737b678255e6ff6337e3b475256b840876`): not applicable on its
  own. Checking it found that this fork's creative `SetCreativeModeSlot` path sent only item, count and
  damage, and the Horizon server read back only damage and enchantments, so a creative move deleted
  names, books, colours and (from vanilla servers) every other component. Fixed independently: the
  client returns a received stack's wire form, and the server reads back what it writes. The Horizon
  server still sends no `bees` component, so hives keep their bees only on vanilla servers.
- Shulker knockback (`52c6277ed9e5736e9a78a07db501db11c3f6ecff`): parity only; the fork's shulker tick
  already zeroes its velocity. Not taken.

## Maintaining the record

1. Fetch upstream without replacing this fork's branch:

   ```sh
   git fetch https://github.com/horizon-rust/horizon.git main:refs/remotes/upstream/main
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
