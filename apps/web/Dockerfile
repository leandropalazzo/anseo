## Build stage — uses pnpm + the workspace's lockfile.
FROM node:22-bookworm-slim AS builder
RUN corepack enable
WORKDIR /app
COPY apps/web/package.json apps/web/pnpm-lock.yaml apps/web/pnpm-workspace.yaml ./
RUN pnpm install --frozen-lockfile
COPY apps/web/ .
ENV NEXT_TELEMETRY_DISABLED=1
RUN pnpm build

## Runtime — slim, calls `next` directly so we avoid the pnpm deps-status
## check that pnpm 11.3+ runs on every invocation (and which needs write
## access to /app).
FROM node:22-bookworm-slim
# wget for the compose healthcheck.
RUN apt-get update \
 && apt-get install -y --no-install-recommends wget \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /app
ENV NODE_ENV=production
ENV NEXT_TELEMETRY_DISABLED=1
ENV PORT=5173
ENV HOSTNAME=0.0.0.0
COPY --from=builder --chown=1000:1000 /app/.next ./.next
COPY --from=builder --chown=1000:1000 /app/public ./public
COPY --from=builder --chown=1000:1000 /app/package.json ./package.json
COPY --from=builder --chown=1000:1000 /app/node_modules ./node_modules
EXPOSE 5173
USER 1000:1000
CMD ["node", "node_modules/next/dist/bin/next", "start", "--hostname", "0.0.0.0", "--port", "5173"]
