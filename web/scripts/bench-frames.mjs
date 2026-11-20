export function watchFrameRate(page, { tail, strict, expectedChunks, verbose }) {
  const samples = [], start = performance.now();
  const elapsed = () => (performance.now() - start) / 1000;
  let runtimeError;
  page.on("console", (message) => {
    const text = message.text(), seconds = elapsed();
    if (text.includes("Horizon: the game stopped:")) runtimeError ??= new Error(text);
    const status = text.match(/Horizon web: frame (\d+).*?chunks (\d+)/);
    if (status) {
      const settled = text.match(/screen Playing title false paused false .*?chunks ([1-9]\d* generated \d+ meshed [1-9]\d*) loading false entry false player Some/);
      samples.push({ seconds, frame: Number(status[1]), chunks: Number(status[2]), settled: settled?.[1] });
    }
    if (verbose || /error|panic|failed/i.test(text)) console.log(`[${seconds.toFixed(1)}] ${text.slice(0, 300)}`);
  });
  page.on("pageerror", (error) => {
    runtimeError ??= error;
    console.log("pageerror", error.message);
  });

  const fps = (first, last) => (last.frame - first.frame) / (last.seconds - first.seconds);
  return (loaderError) => {
    if (loaderError) console.log(`error: ${loaderError}`);
    const rates = samples.slice(1).map((sample, index) => fps(samples[index], sample).toFixed(1));
    console.log(`fps per status line: ${rates.join(" ")}`);
    const last = samples.slice(-(tail + 1));
    if (strict || expectedChunks !== undefined) {
      if (runtimeError) throw runtimeError;
      if (loaderError) throw new Error(loaderError);
      if (last.length < 2 || last.length !== tail + 1 || !last[0].settled ||
          last.some((sample) => sample.settled !== last[0].settled) ||
          last.slice(1).some((sample, index) => sample.frame <= last[index].frame) ||
          elapsed() - last.at(-1).seconds > 5) {
        throw new Error("The game did not keep rendering a settled world through the measurement window");
      }
      if (expectedChunks !== undefined && last[0].chunks !== Number(expectedChunks)) {
        throw new Error(`Expected ${expectedChunks} loaded chunks, got ${last[0].chunks}`);
      }
    }
    if (last.length > 1) console.log(`steady fps (last ${last.length - 1} intervals): ${fps(last[0], last.at(-1)).toFixed(2)}`);
  };
}
