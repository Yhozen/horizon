export const filenames = {
  javascript: ["horizon-client.js", "text/javascript"],
  wasm: ["horizon-client_bg.wasm", "application/wasm"],
  resources: ["horizon-data.bin", "application/octet-stream"],
};

/** Assets a release may have: the private preview's copy of the open bundle that
 * its players wear their own art over on the shared server (docs/open-creative.md). */
export const optionalFilenames = {
  sharedResources: ["horizon-open-data.bin", "application/octet-stream"],
  /** A shader pack the page puts in the game's `shaderpacks` folder (web/shaderpacks). */
  shaderPack: ["miniature-shader-2.19.zip", "application/zip"],
};

function checkAsset(release, name, filename, contentType) {
  const asset = release.assets?.[name];
  if (!asset || !/^[a-f0-9]{64}$/.test(asset.sha256) ||
      !Number.isSafeInteger(asset.size) || asset.size <= 0 ||
      asset.url !== `/assets/${asset.sha256}/${filename}` || asset.contentType !== contentType) {
    throw new Error(`Invalid ${name} asset in release`);
  }
}

export function validateRelease(release) {
  if (release?.format !== 1 || !/^[a-f0-9]{64}$/.test(release.id)) {
    throw new Error("Unsupported or invalid game release");
  }
  for (const [name, [filename, contentType]] of Object.entries(filenames)) checkAsset(release, name, filename, contentType);
  for (const [name, [filename, contentType]] of Object.entries(optionalFilenames)) {
    if (release.assets && name in release.assets) checkAsset(release, name, filename, contentType);
  }
  return release;
}
