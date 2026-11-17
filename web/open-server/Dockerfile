# Horizon Open Creative's shared server (docs/open-creative.md): Horizon's own
# dedicated server (`horizon-client --dedicated`, built without audio or
# controllers) on the open bundle. `pnpm deploy:open` stages this folder with
# the binary and the bundle, then Cloudflare runs it behind the open Worker.
FROM debian:bookworm-slim
RUN useradd --create-home --uid 1000 horizon && mkdir -p /srv/horizon && chown horizon /srv/horizon
COPY --chown=root:root horizon-client /usr/local/bin/horizon-client
COPY --chown=root:root resources /opt/horizon/resources
COPY --chown=horizon:horizon server.properties eula.txt /srv/horizon/
USER horizon
WORKDIR /srv/horizon
ENV HORIZON_HOME=/srv/horizon/home HORIZON_NO_AUDIO=1 HORIZON_LAUNCHER_NO_SELF_UPDATE=1
EXPOSE 25565
# The console reads standard input; the container gives it none.
CMD ["horizon-client", "--resources", "/opt/horizon/resources", "--dedicated", "--nogui"]
