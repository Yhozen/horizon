export const filenames = {
  javascript: ["horizon-client.js", "text/javascript"],
  wasm: ["horizon-client_bg.wasm", "application/wasm"],
  resources: ["horizon-data.bin", "application/octet-stream"],
};

export function validateRelease(release) {
  if (release?.format !== 1 || !/^[a-f0-9]{64}$/.test(release.id)) {
    throw new Error("Unsupported or invalid game release");
  }
  for (const [name, [filename, contentType]] of Object.entries(filenames)) {
    const asset = release.assets?.[name];
    if (!asset || !/^[a-f0-9]{64}$/.test(asset.sha256) ||
        !Number.isSafeInteger(asset.size) || asset.size <= 0 ||
        asset.url !== `/assets/${asset.sha256}/${filename}` || asset.contentType !== contentType) {
      throw new Error(`Invalid ${name} asset in release`);
    }
  }
  return release;
}
