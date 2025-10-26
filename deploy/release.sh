#!/usr/bin/env bash

set -euo pipefail

if ! command -v docker >/dev/null 2>&1; then
  echo "docker is required on the build host" >&2
  exit 1
fi

: "${REGISTRY:?Set REGISTRY to the target registry (e.g., registry.digitalocean.com/theimposter)}"
: "${SSH_HOST:?Set SSH_HOST to the droplet hostname or IP}"
SSH_USER="${SSH_USER:-root}"
IMAGE_TAG="${IMAGE_TAG:-$(git rev-parse --short HEAD)}"
PLATFORM="${PLATFORM:-linux/amd64}"
DEPLOY_PATH="${DEPLOY_PATH:-/opt/theimposter}"
DOMAIN="${DOMAIN:-theimposter.fun}"

echo "Building backend image ${REGISTRY}/backend:${IMAGE_TAG}..."
docker buildx build \
  --platform "${PLATFORM}" \
  --push \
  -t "${REGISTRY}/backend:${IMAGE_TAG}" \
  backend

echo "Building frontend image ${REGISTRY}/frontend:${IMAGE_TAG}..."
docker buildx build \
  --platform "${PLATFORM}" \
  --push \
  -f deploy/caddy/Dockerfile \
  -t "${REGISTRY}/frontend:${IMAGE_TAG}" \
  .

echo "Syncing compose bundle to ${SSH_HOST}:${DEPLOY_PATH}..."
rsync -az --delete \
  docker-compose.yml \
  deploy/ \
  "${SSH_USER}@${SSH_HOST}:${DEPLOY_PATH}/"

echo "Rolling out ${IMAGE_TAG} on ${SSH_HOST}..."
ssh "${SSH_USER}@${SSH_HOST}" <<EOF
  set -euo pipefail
  mkdir -p "${DEPLOY_PATH}"
  cd "${DEPLOY_PATH}"
  export REGISTRY="${REGISTRY}"
  export IMAGE_TAG="${IMAGE_TAG}"
  export DOMAIN="${DOMAIN}"
  docker compose pull
  docker compose up -d --remove-orphans
EOF

echo "Deployment complete. Active tag: ${IMAGE_TAG}"
