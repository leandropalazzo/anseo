# Docker Infrastructure

Phase 1 target: Docker Compose for the local OpenGEO stack.

Planned services:

- `api`
- `worker`
- `web`
- `postgres` using PostgreSQL 16
- `redis` using Redis 7.x

The completed Phase 1 stack must bind API and dashboard surfaces to localhost by default, run database migrations before app traffic, and boot healthy within 60 seconds on a host with at least 2 CPU and 4 GB RAM.

Compose files land in a later Foundation story once health endpoints and migration entrypoints exist.
