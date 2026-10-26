export async function closeSocket(socket?: Pick<Socket, "close">) {
  // Native container sockets may throw synchronously when their port is not ready.
  try { await socket?.close(); } catch {}
}

// Protocol 777: status handshake for localhost:25565, then StatusRequest. A native
// container socket's opened promise can resolve before its first I/O reports that
// Java isn't listening, so readiness needs a real response, on a separate socket.
const statusRequest = new Uint8Array([16, 0, 137, 6, 9, 108, 111, 99, 97, 108, 104, 111, 115, 116, 99, 221, 1, 1, 0]);
async function checkReady(socket: Socket) {
  const writer = socket.writable.getWriter();
  try { await writer.write(statusRequest); } finally { writer.releaseLock(); }
  const reader = socket.readable.getReader();
  try {
    const { value, done } = await reader.read();
    if (done || !value?.byteLength) throw new Error("Minecraft did not answer its status request");
  } finally { reader.releaseLock(); }
}

export async function openMinecraftPort(container: Container, attempts = 360, retryMs = 500): Promise<Socket> {
  if (!container.running) container.start({ enableInternet: true });
  await container.setInactivityTimeout(5 * 60 * 1000);
  let lastError: unknown;
  const deadline = Date.now() + 180_000;
  for (let attempt = 0; attempt < attempts && Date.now() < deadline; attempt++) {
    let socket: Socket | undefined;
    let timer: ReturnType<typeof setTimeout> | undefined;
    try {
      socket = container.getTcpPort(25565).connect("127.0.0.1:25565");
      // A failed connection rejects both promises, even if only opened was awaited.
      void socket.closed.catch(() => {});
      await socket.opened;
      await Promise.race([checkReady(socket), new Promise((_, reject) => {
        timer = setTimeout(() => reject(new Error("Minecraft status is not ready")), Math.min(2000, deadline - Date.now()));
      })]);
      await closeSocket(socket);
      socket = container.getTcpPort(25565).connect("127.0.0.1:25565");
      void socket.closed.catch(() => {});
      await socket.opened;
      return socket;
    } catch (error) { lastError = error; await closeSocket(socket); }
    finally { clearTimeout(timer); }
    if (attempt + 1 < attempts) await new Promise((resolve) => setTimeout(resolve, retryMs));
  }
  throw new Error(`Minecraft server startup timed out: ${lastError}`);
}
