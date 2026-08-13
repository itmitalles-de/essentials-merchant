import process from 'node:process';

let input = '';
for await (const chunk of process.stdin) input += chunk;
const compose = JSON.parse(input);
const services = Object.fromEntries(
    Object.entries(compose.services ?? {}).map(([name, service]) => [name, {
        image: service.image ?? null,
        build: service.build ? true : false,
        environment_keys: Object.keys(service.environment ?? {}).sort(),
        mounts: (service.volumes ?? []).map(volume => ({
            type: volume.type ?? 'volume',
            target: volume.target ?? null,
            source_kind: volume.type === 'bind' ? 'redacted-bind' : 'named-volume',
        })),
        published_ports: (service.ports ?? []).map(port => ({
            target: port.target ?? null,
            protocol: port.protocol ?? 'tcp',
        })),
    }]),
);
process.stdout.write(`${JSON.stringify({
    name: compose.name ?? null,
    services,
    volumes: Object.keys(compose.volumes ?? {}).sort(),
    networks: Object.keys(compose.networks ?? {}).sort(),
    redaction: 'Environment values, host paths, credentials and tokens are intentionally omitted.',
}, null, 2)}\n`);
