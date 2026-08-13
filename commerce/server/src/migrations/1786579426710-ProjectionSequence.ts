import {MigrationInterface, QueryRunner} from "typeorm";

export class ProjectionSequence1786579426710 implements MigrationInterface {

   public async up(queryRunner: QueryRunner): Promise<any> {
        await queryRunner.query(`ALTER TABLE "product_variant" ADD "customFieldsCoreprojectionsequence" integer`, undefined);
   }

   public async down(queryRunner: QueryRunner): Promise<any> {
        await queryRunner.query(`ALTER TABLE "product_variant" DROP COLUMN "customFieldsCoreprojectionsequence"`, undefined);
   }

}
