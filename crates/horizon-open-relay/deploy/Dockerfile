# The open edition's relay to a Java server (docs/open-creative.md, "Joining a
# Java server through the relay"). The image holds no Mojang file: on first
# start the relay downloads Mojang's official 26.3 server jar (checked against
# the SHA-1 in Mojang's version manifest), runs its data generator once and
# keeps only the two reports it reads, in the container's own disk.
#
# Stage this folder's Dockerfile with the relay binary and the open bundle the
# site was packed from (the brand players check is derived from it):
#
#   cargo build --release -p horizon-open-relay
#   mkdir -p stage && cp crates/horizon-open-relay/deploy/Dockerfile stage/
#   cp target/release/horizon-open-relay stage/
#   cp -r web/.build-open/resources stage/resources    # or: horizon-prepare open --out stage/resources
#   docker build -t horizon-open-relay stage
#
# Run: docker run -e HORIZON_RELAY_UPSTREAM=survival.minecraft.rccpr.tech:25565 -p 25565:25565 horizon-open-relay
FROM eclipse-temurin:25-jre
RUN useradd --create-home --uid 1001 relay && mkdir -p /srv/relay && chown relay /srv/relay
COPY --chown=root:root horizon-open-relay /usr/local/bin/horizon-open-relay
COPY --chown=root:root resources /opt/horizon/resources
USER relay
WORKDIR /srv/relay
ENV HORIZON_RELAY_LISTEN=0.0.0.0:25565 \
    HORIZON_RELAY_OPEN_RESOURCES=/opt/horizon/resources \
    HORIZON_RELAY_VANILLA_DATA=/srv/relay/vanilla
EXPOSE 25565
CMD ["horizon-open-relay", "serve"]
