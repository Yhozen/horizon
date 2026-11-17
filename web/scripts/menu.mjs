// The landing menu (web/menu) for one site: menu.js, the Foldkit program bundled with
// rolldown, and menu.css with the theme's fonts, icons and that site's art inlined, so
// both stay small text files the Workers serve from their shell.
import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { build } from "rolldown";

const web = fileURLToPath(new URL("..", import.meta.url));
const repo = join(web, "..");
/** The menu's icon names, and the theme's pixel icons they use. */
const ICONS = { info: "info-box", monitor: "monitor", user: "user", play: "play", globe: "globe", bug: "bug", heart: "heart" };
const FONTS = [["Geist-Regular.woff2", 400], ["Geist-Medium.woff2", 500]];
/** Each site's pictures: the private one may show Minecraft's look, the open one never. */
export const ART = {
  horizon: { village: "village.webp", night: "night.webp", sunset: "sunset.webp" },
  open: { meadow: "open-meadow.webp" },
};

const dataUrl = async (path, type) => `url("data:${type};base64,${(await readFile(path)).toString("base64")}")`;

export async function bundleMenu(edition) {
  if (!ART[edition]) throw new Error(`Unknown edition ${edition}`);
  const result = await build({ input: join(web, "menu/entry.ts"), platform: "browser", write: false,
    output: { format: "esm", minify: true, codeSplitting: false } });
  const js = result.output.find((file) => file.type === "chunk").code;
  const css = [];
  for (const [file, weight] of FONTS) {
    css.push(`@font-face{font-family:Geist;font-weight:${weight};font-display:swap;src:${
      await dataUrl(join(repo, "website/public/fonts", file), "font/woff2")} format("woff2")}`);
  }
  for (const [name, file] of Object.entries(ICONS)) {
    css.push(`.icon-${name}{--icon:${await dataUrl(join(repo, "crates/horizon-theme/icons/pixel", `${file}.svg`), "image/svg+xml")}}`);
  }
  for (const [key, file] of Object.entries(ART[edition])) {
    css.push(`.art-${key}{background-image:${await dataUrl(join(web, "menu/art", file), "image/webp")}}`);
  }
  css.push(await readFile(join(web, "menu/menu.css"), "utf8"));
  return { js, css: css.join("\n") };
}
