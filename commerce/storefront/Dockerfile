FROM node:22-alpine@sha256:c610fcdfb1d5b4740dd70c284ed3cb16bb857e0f7166196e36a5501df7a3aa32 AS dependencies
ENV NEXT_TELEMETRY_DISABLED=1
WORKDIR /app
COPY package.json package-lock.json ./
COPY server/package.json server/package.json
COPY storefront/package.json storefront/package.json
RUN npm ci --workspace storefront --include-workspace-root

FROM dependencies AS build
COPY storefront storefront
WORKDIR /app/storefront
RUN npm run build

FROM node:22-alpine@sha256:c610fcdfb1d5b4740dd70c284ed3cb16bb857e0f7166196e36a5501df7a3aa32 AS runtime
ENV NODE_ENV=production \
    NEXT_TELEMETRY_DISABLED=1 \
    HOSTNAME=0.0.0.0 \
    PORT=3000
WORKDIR /app
RUN addgroup -S shop && adduser -S shop -G shop
COPY --from=build --chown=shop:shop /app/storefront/.next/standalone ./
COPY --from=build --chown=shop:shop /app/storefront/.next/static ./storefront/.next/static
USER shop
EXPOSE 3000
CMD ["node", "storefront/server.js"]
