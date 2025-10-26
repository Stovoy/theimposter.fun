FROM node:22-alpine AS frontend-builder

WORKDIR /frontend

COPY frontend/package*.json ./
RUN npm ci

COPY frontend ./
RUN npm run build

FROM caddy:2.9-alpine

COPY deploy/caddy/Caddyfile /etc/caddy/Caddyfile
COPY --from=frontend-builder /frontend/dist /srv/static

EXPOSE 80 443
CMD ["caddy", "run", "--config", "/etc/caddy/Caddyfile", "--adapter", "caddyfile"]
