ALTER TABLE "User" ADD COLUMN "iapSubject" text;
CREATE UNIQUE INDEX "User_iapSubject_unique" ON "User" USING btree ("iapSubject");
