FROM node:22-bookworm-slim AS dependencies
WORKDIR /app
COPY package.json package-lock.json ./
COPY server/package.json server/package.json
COPY storefront/package.json storefront/package.json
RUN npm ci --workspace server --include-workspace-root

FROM dependencies AS build
COPY server server
WORKDIR /app/server
RUN npm run build

FROM node:22-bookworm-slim AS runtime
ENV NODE_ENV=production
WORKDIR /app
RUN groupadd --system shop && useradd --system --gid shop --create-home shop
COPY package.json package-lock.json ./
COPY server/package.json server/package.json
COPY storefront/package.json storefront/package.json
RUN npm ci --omit=dev --workspace server --include-workspace-root && npm cache clean --force
COPY --from=build --chown=shop:shop /app/server/dist ./server/dist
COPY --from=build --chown=shop:shop /app/server/static ./server/static
WORKDIR /app/server
USER shop
EXPOSE 3000
CMD ["npm", "run", "start:server"]
