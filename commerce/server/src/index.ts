import { bootstrap, runMigrations } from '@vendure/core';
import { config } from './vendure-config';

runMigrations(config)
    .then(() => bootstrap(config))
    .catch(error => {
        console.error(error);
        process.exitCode = 1;
    });
