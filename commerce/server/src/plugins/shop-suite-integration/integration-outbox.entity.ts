import { Column, Entity, Index } from 'typeorm';
import { VendureEntity } from '@vendure/core';
import { DeepPartial } from '@vendure/common/lib/shared-types';

export type IntegrationOutboxStatus = 'pending' | 'processing' | 'delivered' | 'dead';

@Entity('shop_suite_integration_outbox')
@Index(['source', 'eventId'], { unique: true })
export class IntegrationOutbox extends VendureEntity {
    constructor(input?: DeepPartial<IntegrationOutbox>) {
        super(input);
    }

    @Column()
    source: string;

    @Column()
    eventId: string;

    @Column()
    eventType: string;

    @Column({ type: 'jsonb' })
    payload: Record<string, unknown>;

    @Column({ default: 'pending' })
    status: IntegrationOutboxStatus;

    @Column({ default: 0 })
    attempts: number;

    @Column({ type: 'timestamptz', default: () => 'now()' })
    availableAt: Date;

    @Column({ type: 'timestamptz', nullable: true })
    lockedAt: Date | null;

    @Column({ type: 'text', nullable: true })
    lastError: string | null;

    @Column({ type: 'timestamptz', nullable: true })
    deliveredAt: Date | null;
}
