/* 
  Postgres system table DDLs

  extracted from docker image.
*/ 

CREATE DOMAIN "information_schema"."sql_identifier" as name;

CREATE DOMAIN "information_schema"."cardinal_number" as integer;

CREATE DOMAIN "information_schema"."character_data" as character varying;

CREATE DOMAIN "information_schema"."yes_or_no" as character varying;

CREATE DOMAIN "information_schema"."time_stamp" as timestamp with time zone;


-- sql_features table
CREATE TABLE "information_schema"."sql_features" (
    "feature_id" information_schema.character_data,
    "feature_name" information_schema.character_data,
    "sub_feature_id" information_schema.character_data,
    "sub_feature_name" information_schema.character_data,
    "is_supported" information_schema.yes_or_no,
    "is_verified_by" information_schema.character_data,
    "comments" information_schema.character_data
);

-- sql_implementation_info table
CREATE TABLE "information_schema"."sql_implementation_info" (
    "implementation_info_id" information_schema.character_data,
    "implementation_info_name" information_schema.character_data,
    "integer_value" information_schema.cardinal_number,
    "character_value" information_schema.character_data,
    "comments" information_schema.character_data
);

-- sql_parts table
CREATE TABLE "information_schema"."sql_parts" (
    "feature_id" information_schema.character_data,
    "feature_name" information_schema.character_data,
    "is_supported" information_schema.yes_or_no,
    "is_verified_by" information_schema.character_data,
    "comments" information_schema.character_data
);

-- sql_sizing table
CREATE TABLE "information_schema"."sql_sizing" (
    "sizing_id" information_schema.cardinal_number,
    "sizing_name" information_schema.character_data,
    "supported_value" information_schema.cardinal_number,
    "comments" information_schema.character_data
);

-- pg_aggregate table
CREATE TABLE "pg_catalog"."pg_aggregate" (
    "aggfnoid" regproc not null,
    "aggkind" "char" not null,
    "aggnumdirectargs" smallint not null,
    "aggtransfn" regproc not null,
    "aggfinalfn" regproc not null,
    "aggcombinefn" regproc not null,
    "aggserialfn" regproc not null,
    "aggdeserialfn" regproc not null,
    "aggmtransfn" regproc not null,
    "aggminvtransfn" regproc not null,
    "aggmfinalfn" regproc not null,
    "aggfinalextra" boolean not null,
    "aggmfinalextra" boolean not null,
    "aggfinalmodify" "char" not null,
    "aggmfinalmodify" "char" not null,
    "aggsortop" oid not null,
    "aggtranstype" oid not null,
    "aggtransspace" integer not null,
    "aggmtranstype" oid not null,
    "aggmtransspace" integer not null,
    "agginitval" text,
    "aggminitval" text
);

-- pg_am table
CREATE TABLE "pg_catalog"."pg_am" (
    "oid" oid not null,
    "amname" name not null,
    "amhandler" regproc not null,
    "amtype" "char" not null
);

-- pg_amop table
CREATE TABLE "pg_catalog"."pg_amop" (
    "oid" oid not null,
    "amopfamily" oid not null,
    "amoplefttype" oid not null,
    "amoprighttype" oid not null,
    "amopstrategy" smallint not null,
    "amoppurpose" "char" not null,
    "amopopr" oid not null,
    "amopmethod" oid not null,
    "amopsortfamily" oid not null
);

-- pg_amproc table
CREATE TABLE "pg_catalog"."pg_amproc" (
    "oid" oid not null,
    "amprocfamily" oid not null,
    "amproclefttype" oid not null,
    "amprocrighttype" oid not null,
    "amprocnum" smallint not null,
    "amproc" regproc not null
);

-- pg_attrdef table
CREATE TABLE "pg_catalog"."pg_attrdef" (
    "oid" oid not null,
    "adrelid" oid not null,
    "adnum" smallint not null,
    "adbin" pg_node_tree not null
);

-- pg_attribute table
CREATE TABLE "pg_catalog"."pg_attribute" (
    "attrelid" oid not null,
    "attname" name not null,
    "atttypid" oid not null,
    "attlen" smallint not null,
    "attnum" smallint not null,
    "attcacheoff" integer not null,
    "atttypmod" integer not null,
    "attndims" smallint not null,
    "attbyval" boolean not null,
    "attalign" "char" not null,
    "attstorage" "char" not null,
    "attcompression" "char" not null,
    "attnotnull" boolean not null,
    "atthasdef" boolean not null,
    "atthasmissing" boolean not null,
    "attidentity" "char" not null,
    "attgenerated" "char" not null,
    "attisdropped" boolean not null,
    "attislocal" boolean not null,
    "attinhcount" smallint not null,
    "attcollation" oid not null,
    "attstattarget" smallint,
    "attacl" aclitem[],
    "attoptions" text[],
    "attfdwoptions" text[],
    "attmissingval" anyarray
);

-- pg_auth_members table
CREATE TABLE "pg_catalog"."pg_auth_members" (
    "oid" oid not null,
    "roleid" oid not null,
    "member" oid not null,
    "grantor" oid not null,
    "admin_option" boolean not null,
    "inherit_option" boolean not null,
    "set_option" boolean not null
);

-- pg_authid table
CREATE TABLE "pg_catalog"."pg_authid" (
    "oid" oid not null,
    "rolname" name not null,
    "rolsuper" boolean not null,
    "rolinherit" boolean not null,
    "rolcreaterole" boolean not null,
    "rolcreatedb" boolean not null,
    "rolcanlogin" boolean not null,
    "rolreplication" boolean not null,
    "rolbypassrls" boolean not null,
    "rolconnlimit" integer not null,
    "rolpassword" text,
    "rolvaliduntil" timestamp with time zone
);

-- pg_cast table
CREATE TABLE "pg_catalog"."pg_cast" (
    "oid" oid not null,
    "castsource" oid not null,
    "casttarget" oid not null,
    "castfunc" oid not null,
    "castcontext" "char" not null,
    "castmethod" "char" not null
);

-- pg_class table
CREATE TABLE "pg_catalog"."pg_class" (
    "oid" oid not null,
    "relname" name not null,
    "relnamespace" oid not null,
    "reltype" oid not null,
    "reloftype" oid not null,
    "relowner" oid not null,
    "relam" oid not null,
    "relfilenode" oid not null,
    "reltablespace" oid not null,
    "relpages" integer not null,
    "reltuples" real not null,
    "relallvisible" integer not null,
    "reltoastrelid" oid not null,
    "relhasindex" boolean not null,
    "relisshared" boolean not null,
    "relpersistence" "char" not null,
    "relkind" "char" not null,
    "relnatts" smallint not null,
    "relchecks" smallint not null,
    "relhasrules" boolean not null,
    "relhastriggers" boolean not null,
    "relhassubclass" boolean not null,
    "relrowsecurity" boolean not null,
    "relforcerowsecurity" boolean not null,
    "relispopulated" boolean not null,
    "relreplident" "char" not null,
    "relispartition" boolean not null,
    "relrewrite" oid not null,
    "relfrozenxid" xid not null,
    "relminmxid" xid not null,
    "relacl" aclitem[],
    "reloptions" text[],
    "relpartbound" pg_node_tree
);

-- pg_collation table
CREATE TABLE "pg_catalog"."pg_collation" (
    "oid" oid not null,
    "collname" name not null,
    "collnamespace" oid not null,
    "collowner" oid not null,
    "collprovider" "char" not null,
    "collisdeterministic" boolean not null,
    "collencoding" integer not null,
    "collcollate" text,
    "collctype" text,
    "colllocale" text,
    "collicurules" text,
    "collversion" text
);

-- pg_constraint table
CREATE TABLE "pg_catalog"."pg_constraint" (
    "oid" oid not null,
    "conname" name not null,
    "connamespace" oid not null,
    "contype" "char" not null,
    "condeferrable" boolean not null,
    "condeferred" boolean not null,
    "convalidated" boolean not null,
    "conrelid" oid not null,
    "contypid" oid not null,
    "conindid" oid not null,
    "conparentid" oid not null,
    "confrelid" oid not null,
    "confupdtype" "char" not null,
    "confdeltype" "char" not null,
    "confmatchtype" "char" not null,
    "conislocal" boolean not null,
    "coninhcount" smallint not null,
    "connoinherit" boolean not null,
    "conkey" smallint[],
    "confkey" smallint[],
    "conpfeqop" oid[],
    "conppeqop" oid[],
    "conffeqop" oid[],
    "confdelsetcols" smallint[],
    "conexclop" oid[],
    "conbin" pg_node_tree
);

-- pg_conversion table
CREATE TABLE "pg_catalog"."pg_conversion" (
    "oid" oid not null,
    "conname" name not null,
    "connamespace" oid not null,
    "conowner" oid not null,
    "conforencoding" integer not null,
    "contoencoding" integer not null,
    "conproc" regproc not null,
    "condefault" boolean not null
);

-- pg_database table
CREATE TABLE "pg_catalog"."pg_database" (
    "oid" oid not null,
    "datname" name not null,
    "datdba" oid not null,
    "encoding" integer not null,
    "datlocprovider" "char" not null,
    "datistemplate" boolean not null,
    "datallowconn" boolean not null,
    "dathasloginevt" boolean not null,
    "datconnlimit" integer not null,
    "datfrozenxid" xid not null,
    "datminmxid" xid not null,
    "dattablespace" oid not null,
    "datcollate" text not null,
    "datctype" text not null,
    "datlocale" text,
    "daticurules" text,
    "datcollversion" text,
    "datacl" aclitem[]
);

-- pg_db_role_setting table
CREATE TABLE "pg_catalog"."pg_db_role_setting" (
    "setdatabase" oid not null,
    "setrole" oid not null,
    "setconfig" text[]
);

-- pg_default_acl table
CREATE TABLE "pg_catalog"."pg_default_acl" (
    "oid" oid not null,
    "defaclrole" oid not null,
    "defaclnamespace" oid not null,
    "defaclobjtype" "char" not null,
    "defaclacl" aclitem[] not null
);

-- pg_depend table
CREATE TABLE "pg_catalog"."pg_depend" (
    "classid" oid not null,
    "objid" oid not null,
    "objsubid" integer not null,
    "refclassid" oid not null,
    "refobjid" oid not null,
    "refobjsubid" integer not null,
    "deptype" "char" not null
);

-- pg_description table
CREATE TABLE "pg_catalog"."pg_description" (
    "objoid" oid not null,
    "classoid" oid not null,
    "objsubid" integer not null,
    "description" text not null
);

-- pg_enum table
CREATE TABLE "pg_catalog"."pg_enum" (
    "oid" oid not null,
    "enumtypid" oid not null,
    "enumsortorder" real not null,
    "enumlabel" name not null
);

-- pg_event_trigger table
CREATE TABLE "pg_catalog"."pg_event_trigger" (
    "oid" oid not null,
    "evtname" name not null,
    "evtevent" name not null,
    "evtowner" oid not null,
    "evtfoid" oid not null,
    "evtenabled" "char" not null,
    "evttags" text[]
);

-- pg_extension table
CREATE TABLE "pg_catalog"."pg_extension" (
    "oid" oid not null,
    "extname" name not null,
    "extowner" oid not null,
    "extnamespace" oid not null,
    "extrelocatable" boolean not null,
    "extversion" text not null,
    "extconfig" oid[],
    "extcondition" text[]
);

-- pg_foreign_data_wrapper table
CREATE TABLE "pg_catalog"."pg_foreign_data_wrapper" (
    "oid" oid not null,
    "fdwname" name not null,
    "fdwowner" oid not null,
    "fdwhandler" oid not null,
    "fdwvalidator" oid not null,
    "fdwacl" aclitem[],
    "fdwoptions" text[]
);

-- pg_foreign_server table
CREATE TABLE "pg_catalog"."pg_foreign_server" (
    "oid" oid not null,
    "srvname" name not null,
    "srvowner" oid not null,
    "srvfdw" oid not null,
    "srvtype" text,
    "srvversion" text,
    "srvacl" aclitem[],
    "srvoptions" text[]
);

-- pg_foreign_table table
CREATE TABLE "pg_catalog"."pg_foreign_table" (
    "ftrelid" oid not null,
    "ftserver" oid not null,
    "ftoptions" text[]
);

-- pg_index table
CREATE TABLE "pg_catalog"."pg_index" (
    "indexrelid" oid not null,
    "indrelid" oid not null,
    "indnatts" smallint not null,
    "indnkeyatts" smallint not null,
    "indisunique" boolean not null,
    "indnullsnotdistinct" boolean not null,
    "indisprimary" boolean not null,
    "indisexclusion" boolean not null,
    "indimmediate" boolean not null,
    "indisclustered" boolean not null,
    "indisvalid" boolean not null,
    "indcheckxmin" boolean not null,
    "indisready" boolean not null,
    "indislive" boolean not null,
    "indisreplident" boolean not null,
    "indkey" int2vector not null,
    "indcollation" oidvector not null,
    "indclass" oidvector not null,
    "indoption" int2vector not null,
    "indexprs" pg_node_tree,
    "indpred" pg_node_tree
);

-- pg_inherits table
CREATE TABLE "pg_catalog"."pg_inherits" (
    "inhrelid" oid not null,
    "inhparent" oid not null,
    "inhseqno" integer not null,
    "inhdetachpending" boolean not null
);

-- pg_init_privs table
CREATE TABLE "pg_catalog"."pg_init_privs" (
    "objoid" oid not null,
    "classoid" oid not null,
    "objsubid" integer not null,
    "privtype" "char" not null,
    "initprivs" aclitem[] not null
);

-- pg_language table
CREATE TABLE "pg_catalog"."pg_language" (
    "oid" oid not null,
    "lanname" name not null,
    "lanowner" oid not null,
    "lanispl" boolean not null,
    "lanpltrusted" boolean not null,
    "lanplcallfoid" oid not null,
    "laninline" oid not null,
    "lanvalidator" oid not null,
    "lanacl" aclitem[]
);

-- pg_largeobject table
CREATE TABLE "pg_catalog"."pg_largeobject" (
    "loid" oid not null,
    "pageno" integer not null,
    "data" bytea not null
);

-- pg_largeobject_metadata table
CREATE TABLE "pg_catalog"."pg_largeobject_metadata" (
    "oid" oid not null,
    "lomowner" oid not null,
    "lomacl" aclitem[]
);

-- pg_namespace table
CREATE TABLE "pg_catalog"."pg_namespace" (
    "oid" oid not null,
    "nspname" name not null,
    "nspowner" oid not null,
    "nspacl" aclitem[]
);

-- pg_opclass table
CREATE TABLE "pg_catalog"."pg_opclass" (
    "oid" oid not null,
    "opcmethod" oid not null,
    "opcname" name not null,
    "opcnamespace" oid not null,
    "opcowner" oid not null,
    "opcfamily" oid not null,
    "opcintype" oid not null,
    "opcdefault" boolean not null,
    "opckeytype" oid not null
);

-- pg_operator table
CREATE TABLE "pg_catalog"."pg_operator" (
    "oid" oid not null,
    "oprname" name not null,
    "oprnamespace" oid not null,
    "oprowner" oid not null,
    "oprkind" "char" not null,
    "oprcanmerge" boolean not null,
    "oprcanhash" boolean not null,
    "oprleft" oid not null,
    "oprright" oid not null,
    "oprresult" oid not null,
    "oprcom" oid not null,
    "oprnegate" oid not null,
    "oprcode" regproc not null,
    "oprrest" regproc not null,
    "oprjoin" regproc not null
);

-- pg_opfamily table
CREATE TABLE "pg_catalog"."pg_opfamily" (
    "oid" oid not null,
    "opfmethod" oid not null,
    "opfname" name not null,
    "opfnamespace" oid not null,
    "opfowner" oid not null
);

-- pg_parameter_acl table
CREATE TABLE "pg_catalog"."pg_parameter_acl" (
    "oid" oid not null,
    "parname" text not null,
    "paracl" aclitem[]
);

-- pg_partitioned_table table
CREATE TABLE "pg_catalog"."pg_partitioned_table" (
    "partrelid" oid not null,
    "partstrat" "char" not null,
    "partnatts" smallint not null,
    "partdefid" oid not null,
    "partattrs" int2vector not null,
    "partclass" oidvector not null,
    "partcollation" oidvector not null,
    "partexprs" pg_node_tree
);

-- pg_policy table
CREATE TABLE "pg_catalog"."pg_policy" (
    "oid" oid not null,
    "polname" name not null,
    "polrelid" oid not null,
    "polcmd" "char" not null,
    "polpermissive" boolean not null,
    "polroles" oid[] not null,
    "polqual" pg_node_tree,
    "polwithcheck" pg_node_tree
);

-- pg_proc table
CREATE TABLE "pg_catalog"."pg_proc" (
    "oid" oid not null,
    "proname" name not null,
    "pronamespace" oid not null,
    "proowner" oid not null,
    "prolang" oid not null,
    "procost" real not null,
    "prorows" real not null,
    "provariadic" oid not null,
    "prosupport" regproc not null,
    "prokind" "char" not null,
    "prosecdef" boolean not null,
    "proleakproof" boolean not null,
    "proisstrict" boolean not null,
    "proretset" boolean not null,
    "provolatile" "char" not null,
    "proparallel" "char" not null,
    "pronargs" smallint not null,
    "pronargdefaults" smallint not null,
    "prorettype" oid not null,
    "proargtypes" oidvector not null,
    "proallargtypes" oid[],
    "proargmodes" "char"[],
    "proargnames" text[],
    "proargdefaults" pg_node_tree,
    "protrftypes" oid[],
    "prosrc" text not null,
    "probin" text,
    "prosqlbody" pg_node_tree,
    "proconfig" text[],
    "proacl" aclitem[]
);

-- pg_publication table
CREATE TABLE "pg_catalog"."pg_publication" (
    "oid" oid not null,
    "pubname" name not null,
    "pubowner" oid not null,
    "puballtables" boolean not null,
    "pubinsert" boolean not null,
    "pubupdate" boolean not null,
    "pubdelete" boolean not null,
    "pubtruncate" boolean not null,
    "pubviaroot" boolean not null
);

-- pg_publication_namespace table
CREATE TABLE "pg_catalog"."pg_publication_namespace" (
    "oid" oid not null,
    "pnpubid" oid not null,
    "pnnspid" oid not null
);

-- pg_publication_rel table
CREATE TABLE "pg_catalog"."pg_publication_rel" (
    "oid" oid not null,
    "prpubid" oid not null,
    "prrelid" oid not null,
    "prqual" pg_node_tree,
    "prattrs" int2vector
);

-- pg_range table
CREATE TABLE "pg_catalog"."pg_range" (
    "rngtypid" oid not null,
    "rngsubtype" oid not null,
    "rngmultitypid" oid not null,
    "rngcollation" oid not null,
    "rngsubopc" oid not null,
    "rngcanonical" regproc not null,
    "rngsubdiff" regproc not null
);

-- pg_replication_origin table
CREATE TABLE "pg_catalog"."pg_replication_origin" (
    "roident" oid not null,
    "roname" text not null
);

-- pg_rewrite table
CREATE TABLE "pg_catalog"."pg_rewrite" (
    "oid" oid not null,
    "rulename" name not null,
    "ev_class" oid not null,
    "ev_type" "char" not null,
    "ev_enabled" "char" not null,
    "is_instead" boolean not null,
    "ev_qual" pg_node_tree not null,
    "ev_action" pg_node_tree not null
);

-- pg_seclabel table
CREATE TABLE "pg_catalog"."pg_seclabel" (
    "objoid" oid not null,
    "classoid" oid not null,
    "objsubid" integer not null,
    "provider" text not null,
    "label" text not null
);

-- pg_sequence table
CREATE TABLE "pg_catalog"."pg_sequence" (
    "seqrelid" oid not null,
    "seqtypid" oid not null,
    "seqstart" bigint not null,
    "seqincrement" bigint not null,
    "seqmax" bigint not null,
    "seqmin" bigint not null,
    "seqcache" bigint not null,
    "seqcycle" boolean not null
);

-- pg_shdepend table
CREATE TABLE "pg_catalog"."pg_shdepend" (
    "dbid" oid not null,
    "classid" oid not null,
    "objid" oid not null,
    "objsubid" integer not null,
    "refclassid" oid not null,
    "refobjid" oid not null,
    "deptype" "char" not null
);

-- pg_shdescription table
CREATE TABLE "pg_catalog"."pg_shdescription" (
    "objoid" oid not null,
    "classoid" oid not null,
    "description" text not null
);

-- pg_shseclabel table
CREATE TABLE "pg_catalog"."pg_shseclabel" (
    "objoid" oid not null,
    "classoid" oid not null,
    "provider" text not null,
    "label" text not null
);

-- pg_statistic table
CREATE TABLE "pg_catalog"."pg_statistic" (
    "starelid" oid not null,
    "staattnum" smallint not null,
    "stainherit" boolean not null,
    "stanullfrac" real not null,
    "stawidth" integer not null,
    "stadistinct" real not null,
    "stakind1" smallint not null,
    "stakind2" smallint not null,
    "stakind3" smallint not null,
    "stakind4" smallint not null,
    "stakind5" smallint not null,
    "staop1" oid not null,
    "staop2" oid not null,
    "staop3" oid not null,
    "staop4" oid not null,
    "staop5" oid not null,
    "stacoll1" oid not null,
    "stacoll2" oid not null,
    "stacoll3" oid not null,
    "stacoll4" oid not null,
    "stacoll5" oid not null,
    "stanumbers1" real[],
    "stanumbers2" real[],
    "stanumbers3" real[],
    "stanumbers4" real[],
    "stanumbers5" real[],
    "stavalues1" anyarray,
    "stavalues2" anyarray,
    "stavalues3" anyarray,
    "stavalues4" anyarray,
    "stavalues5" anyarray
);

-- pg_statistic_ext table
CREATE TABLE "pg_catalog"."pg_statistic_ext" (
    "oid" oid not null,
    "stxrelid" oid not null,
    "stxname" name not null,
    "stxnamespace" oid not null,
    "stxowner" oid not null,
    "stxkeys" int2vector not null,
    "stxstattarget" smallint,
    "stxkind" "char"[] not null,
    "stxexprs" pg_node_tree
);

-- pg_statistic_ext_data table
CREATE TABLE "pg_catalog"."pg_statistic_ext_data" (
    "stxoid" oid not null,
    "stxdinherit" boolean not null,
    "stxdndistinct" pg_ndistinct,
    "stxddependencies" pg_dependencies,
    "stxdmcv" pg_mcv_list,
    "stxdexpr" pg_statistic[]
);

-- pg_subscription table
CREATE TABLE "pg_catalog"."pg_subscription" (
    "oid" oid not null,
    "subdbid" oid not null,
    "subskiplsn" pg_lsn not null,
    "subname" name not null,
    "subowner" oid not null,
    "subenabled" boolean not null,
    "subbinary" boolean not null,
    "substream" "char" not null,
    "subtwophasestate" "char" not null,
    "subdisableonerr" boolean not null,
    "subpasswordrequired" boolean not null,
    "subrunasowner" boolean not null,
    "subfailover" boolean not null,
    "subconninfo" text not null,
    "subslotname" name,
    "subsynccommit" text not null,
    "subpublications" text[] not null,
    "suborigin" text
);

-- pg_subscription_rel table
CREATE TABLE "pg_catalog"."pg_subscription_rel" (
    "srsubid" oid not null,
    "srrelid" oid not null,
    "srsubstate" "char" not null,
    "srsublsn" pg_lsn
);

-- pg_tablespace table
CREATE TABLE "pg_catalog"."pg_tablespace" (
    "oid" oid not null,
    "spcname" name not null,
    "spcowner" oid not null,
    "spcacl" aclitem[],
    "spcoptions" text[]
);

-- pg_transform table
CREATE TABLE "pg_catalog"."pg_transform" (
    "oid" oid not null,
    "trftype" oid not null,
    "trflang" oid not null,
    "trffromsql" regproc not null,
    "trftosql" regproc not null
);

-- pg_trigger table
CREATE TABLE "pg_catalog"."pg_trigger" (
    "oid" oid not null,
    "tgrelid" oid not null,
    "tgparentid" oid not null,
    "tgname" name not null,
    "tgfoid" oid not null,
    "tgtype" smallint not null,
    "tgenabled" "char" not null,
    "tgisinternal" boolean not null,
    "tgconstrrelid" oid not null,
    "tgconstrindid" oid not null,
    "tgconstraint" oid not null,
    "tgdeferrable" boolean not null,
    "tginitdeferred" boolean not null,
    "tgnargs" smallint not null,
    "tgattr" int2vector not null,
    "tgargs" bytea not null,
    "tgqual" pg_node_tree,
    "tgoldtable" name,
    "tgnewtable" name
);

-- pg_ts_config table
CREATE TABLE "pg_catalog"."pg_ts_config" (
    "oid" oid not null,
    "cfgname" name not null,
    "cfgnamespace" oid not null,
    "cfgowner" oid not null,
    "cfgparser" oid not null
);

-- pg_ts_config_map table
CREATE TABLE "pg_catalog"."pg_ts_config_map" (
    "mapcfg" oid not null,
    "maptokentype" integer not null,
    "mapseqno" integer not null,
    "mapdict" oid not null
);

-- pg_ts_dict table
CREATE TABLE "pg_catalog"."pg_ts_dict" (
    "oid" oid not null,
    "dictname" name not null,
    "dictnamespace" oid not null,
    "dictowner" oid not null,
    "dicttemplate" oid not null,
    "dictinitoption" text
);

-- pg_ts_parser table
CREATE TABLE "pg_catalog"."pg_ts_parser" (
    "oid" oid not null,
    "prsname" name not null,
    "prsnamespace" oid not null,
    "prsstart" regproc not null,
    "prstoken" regproc not null,
    "prsend" regproc not null,
    "prsheadline" regproc not null,
    "prslextype" regproc not null
);

-- pg_ts_template table
CREATE TABLE "pg_catalog"."pg_ts_template" (
    "oid" oid not null,
    "tmplname" name not null,
    "tmplnamespace" oid not null,
    "tmplinit" regproc not null,
    "tmpllexize" regproc not null
);

-- pg_type table
CREATE TABLE "pg_catalog"."pg_type" (
    "oid" oid not null,
    "typname" name not null,
    "typnamespace" oid not null,
    "typowner" oid not null,
    "typlen" smallint not null,
    "typbyval" boolean not null,
    "typtype" "char" not null,
    "typcategory" "char" not null,
    "typispreferred" boolean not null,
    "typisdefined" boolean not null,
    "typdelim" "char" not null,
    "typrelid" oid not null,
    "typsubscript" regproc not null,
    "typelem" oid not null,
    "typarray" oid not null,
    "typinput" regproc not null,
    "typoutput" regproc not null,
    "typreceive" regproc not null,
    "typsend" regproc not null,
    "typmodin" regproc not null,
    "typmodout" regproc not null,
    "typanalyze" regproc not null,
    "typalign" "char" not null,
    "typstorage" "char" not null,
    "typnotnull" boolean not null,
    "typbasetype" oid not null,
    "typtypmod" integer not null,
    "typndims" integer not null,
    "typcollation" oid not null,
    "typdefaultbin" pg_node_tree,
    "typdefault" text,
    "typacl" aclitem[]
);

-- pg_user_mapping table
CREATE TABLE "pg_catalog"."pg_user_mapping" (
    "oid" oid not null,
    "umuser" oid not null,
    "umserver" oid not null,
    "umoptions" text[]
);

create function information_schema._pg_expandarray(anyarray, OUT x anyelement, OUT n integer) 
  returns setof record 
  language sql as 
$$
-- 
$$;

create function information_schema._pg_char_max_length(typid oid, typmod integer) 
  returns integer 
  language sql as 
$$
-- 
$$;

create function information_schema._pg_char_octet_length(typid oid, typmod integer) 
  returns integer 
  language sql as 
$$
-- 
$$;

create function information_schema._pg_datetime_precision(typid oid, typmod integer) 
  returns integer 
  language sql as 
$$
-- 
$$;

create function information_schema._pg_index_position(oid, smallint) 
  returns integer 
  language sql as 
$$
-- 
$$;

create function information_schema._pg_interval_type(typid oid, mod integer) 
  returns text 
  language sql as 
$$
-- 
$$;

create function information_schema._pg_numeric_precision(typid oid, typmod integer) 
  returns integer 
  language sql as 
$$
-- 
$$;

create function information_schema._pg_numeric_precision_radix(typid oid, typmod integer) 
  returns integer 
  language sql as 
$$
-- 
$$;

create function information_schema._pg_numeric_scale(typid oid, typmod integer) 
  returns integer 
  language sql as 
$$
-- 
$$;

create function information_schema._pg_truetypid(pg_attribute, pg_type) 
  returns oid 
  language sql as 
$$
-- 
$$;

create function information_schema._pg_truetypmod(pg_attribute, pg_type) 
  returns integer 
  language sql as 
$$
-- 
$$;


CREATE VIEW information_schema._pg_foreign_data_wrappers 
as 
 SELECT w.oid,
    w.fdwowner,
    w.fdwoptions,
    current_database()::information_schema.sql_identifier AS foreign_data_wrapper_catalog,
    w.fdwname::information_schema.sql_identifier AS foreign_data_wrapper_name,
    u.rolname::information_schema.sql_identifier AS authorization_identifier,
    'c'::character varying::information_schema.character_data AS foreign_data_wrapper_language
   FROM pg_foreign_data_wrapper w,
    pg_authid u
  WHERE u.oid = w.fdwowner AND (pg_has_role(w.fdwowner, 'USAGE'::text) OR has_foreign_data_wrapper_privilege(w.oid, 'USAGE'::text));
;

CREATE VIEW information_schema._pg_foreign_servers 
as 
 SELECT s.oid,
    s.srvoptions,
    current_database()::information_schema.sql_identifier AS foreign_server_catalog,
    s.srvname::information_schema.sql_identifier AS foreign_server_name,
    current_database()::information_schema.sql_identifier AS foreign_data_wrapper_catalog,
    w.fdwname::information_schema.sql_identifier AS foreign_data_wrapper_name,
    s.srvtype::information_schema.character_data AS foreign_server_type,
    s.srvversion::information_schema.character_data AS foreign_server_version,
    u.rolname::information_schema.sql_identifier AS authorization_identifier
   FROM pg_foreign_server s,
    pg_foreign_data_wrapper w,
    pg_authid u
  WHERE w.oid = s.srvfdw AND u.oid = s.srvowner AND (pg_has_role(s.srvowner, 'USAGE'::text) OR has_server_privilege(s.oid, 'USAGE'::text));
;

CREATE VIEW information_schema._pg_foreign_table_columns 
as 
 SELECT n.nspname,
    c.relname,
    a.attname,
    a.attfdwoptions
   FROM pg_foreign_table t,
    pg_authid u,
    pg_namespace n,
    pg_class c,
    pg_attribute a
  WHERE u.oid = c.relowner AND (pg_has_role(c.relowner, 'USAGE'::text) OR has_column_privilege(c.oid, a.attnum, 'SELECT, INSERT, UPDATE, REFERENCES'::text)) AND n.oid = c.relnamespace AND c.oid = t.ftrelid AND c.relkind = 'f'::"char" AND a.attrelid = c.oid AND a.attnum > 0;
;

CREATE VIEW information_schema._pg_foreign_tables 
as 
 SELECT current_database()::information_schema.sql_identifier AS foreign_table_catalog,
    n.nspname::information_schema.sql_identifier AS foreign_table_schema,
    c.relname::information_schema.sql_identifier AS foreign_table_name,
    t.ftoptions,
    current_database()::information_schema.sql_identifier AS foreign_server_catalog,
    s.srvname::information_schema.sql_identifier AS foreign_server_name,
    u.rolname::information_schema.sql_identifier AS authorization_identifier
   FROM pg_foreign_table t,
    pg_foreign_server s,
    pg_foreign_data_wrapper w,
    pg_authid u,
    pg_namespace n,
    pg_class c
  WHERE w.oid = s.srvfdw AND u.oid = c.relowner AND (pg_has_role(c.relowner, 'USAGE'::text) OR has_table_privilege(c.oid, 'SELECT, INSERT, UPDATE, DELETE, TRUNCATE, REFERENCES, TRIGGER'::text) OR has_any_column_privilege(c.oid, 'SELECT, INSERT, UPDATE, REFERENCES'::text)) AND n.oid = c.relnamespace AND c.oid = t.ftrelid AND c.relkind = 'f'::"char" AND s.oid = t.ftserver;
;

CREATE VIEW information_schema.applicable_roles 
as 
 SELECT a.rolname::information_schema.sql_identifier AS grantee,
    b.rolname::information_schema.sql_identifier AS role_name,
        CASE
            WHEN m.admin_option THEN 'YES'::text
            ELSE 'NO'::text
        END::information_schema.yes_or_no AS is_grantable
   FROM ( SELECT pg_auth_members.member,
            pg_auth_members.roleid,
            pg_auth_members.admin_option
           FROM pg_auth_members
        UNION
         SELECT pg_database.datdba,
            pg_authid.oid,
            false
           FROM pg_database,
            pg_authid
          WHERE pg_database.datname = current_database() AND pg_authid.rolname = 'pg_database_owner'::name) m
     JOIN pg_authid a ON m.member = a.oid
     JOIN pg_authid b ON m.roleid = b.oid
  WHERE pg_has_role(a.oid, 'USAGE'::text);
;

CREATE VIEW information_schema.attributes 
as 
 SELECT current_database()::information_schema.sql_identifier AS udt_catalog,
    nc.nspname::information_schema.sql_identifier AS udt_schema,
    c.relname::information_schema.sql_identifier AS udt_name,
    a.attname::information_schema.sql_identifier AS attribute_name,
    a.attnum::information_schema.cardinal_number AS ordinal_position,
    pg_get_expr(ad.adbin, ad.adrelid)::information_schema.character_data AS attribute_default,
        CASE
            WHEN a.attnotnull OR t.typtype = 'd'::"char" AND t.typnotnull THEN 'NO'::text
            ELSE 'YES'::text
        END::information_schema.yes_or_no AS is_nullable,
        CASE
            WHEN t.typelem <> 0::oid AND t.typlen = '-1'::integer THEN 'ARRAY'::text
            WHEN nt.nspname = 'pg_catalog'::name THEN format_type(a.atttypid, NULL::integer)
            ELSE 'USER-DEFINED'::text
        END::information_schema.character_data AS data_type,
    information_schema._pg_char_max_length(information_schema._pg_truetypid(a.*, t.*), information_schema._pg_truetypmod(a.*, t.*))::information_schema.cardinal_number AS character_maximum_length,
    information_schema._pg_char_octet_length(information_schema._pg_truetypid(a.*, t.*), information_schema._pg_truetypmod(a.*, t.*))::information_schema.cardinal_number AS character_octet_length,
    NULL::name::information_schema.sql_identifier AS character_set_catalog,
    NULL::name::information_schema.sql_identifier AS character_set_schema,
    NULL::name::information_schema.sql_identifier AS character_set_name,
        CASE
            WHEN nco.nspname IS NOT NULL THEN current_database()
            ELSE NULL::name
        END::information_schema.sql_identifier AS collation_catalog,
    nco.nspname::information_schema.sql_identifier AS collation_schema,
    co.collname::information_schema.sql_identifier AS collation_name,
    information_schema._pg_numeric_precision(information_schema._pg_truetypid(a.*, t.*), information_schema._pg_truetypmod(a.*, t.*))::information_schema.cardinal_number AS numeric_precision,
    information_schema._pg_numeric_precision_radix(information_schema._pg_truetypid(a.*, t.*), information_schema._pg_truetypmod(a.*, t.*))::information_schema.cardinal_number AS numeric_precision_radix,
    information_schema._pg_numeric_scale(information_schema._pg_truetypid(a.*, t.*), information_schema._pg_truetypmod(a.*, t.*))::information_schema.cardinal_number AS numeric_scale,
    information_schema._pg_datetime_precision(information_schema._pg_truetypid(a.*, t.*), information_schema._pg_truetypmod(a.*, t.*))::information_schema.cardinal_number AS datetime_precision,
    information_schema._pg_interval_type(information_schema._pg_truetypid(a.*, t.*), information_schema._pg_truetypmod(a.*, t.*))::information_schema.character_data AS interval_type,
    NULL::integer::information_schema.cardinal_number AS interval_precision,
    current_database()::information_schema.sql_identifier AS attribute_udt_catalog,
    nt.nspname::information_schema.sql_identifier AS attribute_udt_schema,
    t.typname::information_schema.sql_identifier AS attribute_udt_name,
    NULL::name::information_schema.sql_identifier AS scope_catalog,
    NULL::name::information_schema.sql_identifier AS scope_schema,
    NULL::name::information_schema.sql_identifier AS scope_name,
    NULL::integer::information_schema.cardinal_number AS maximum_cardinality,
    a.attnum::information_schema.sql_identifier AS dtd_identifier,
    'NO'::character varying::information_schema.yes_or_no AS is_derived_reference_attribute
   FROM pg_attribute a
     LEFT JOIN pg_attrdef ad ON a.attrelid = ad.adrelid AND a.attnum = ad.adnum
     JOIN (pg_class c
     JOIN pg_namespace nc ON c.relnamespace = nc.oid) ON a.attrelid = c.oid
     JOIN (pg_type t
     JOIN pg_namespace nt ON t.typnamespace = nt.oid) ON a.atttypid = t.oid
     LEFT JOIN (pg_collation co
     JOIN pg_namespace nco ON co.collnamespace = nco.oid) ON a.attcollation = co.oid AND (nco.nspname <> 'pg_catalog'::name OR co.collname <> 'default'::name)
  WHERE a.attnum > 0 AND NOT a.attisdropped AND c.relkind = 'c'::"char" AND (pg_has_role(c.relowner, 'USAGE'::text) OR has_type_privilege(c.reltype, 'USAGE'::text));
;

CREATE VIEW information_schema.character_sets 
as 
 SELECT NULL::name::information_schema.sql_identifier AS character_set_catalog,
    NULL::name::information_schema.sql_identifier AS character_set_schema,
    getdatabaseencoding()::information_schema.sql_identifier AS character_set_name,
        CASE
            WHEN getdatabaseencoding() = 'UTF8'::name THEN 'UCS'::name
            ELSE getdatabaseencoding()
        END::information_schema.sql_identifier AS character_repertoire,
    getdatabaseencoding()::information_schema.sql_identifier AS form_of_use,
    current_database()::information_schema.sql_identifier AS default_collate_catalog,
    nc.nspname::information_schema.sql_identifier AS default_collate_schema,
    c.collname::information_schema.sql_identifier AS default_collate_name
   FROM pg_database d
     LEFT JOIN (pg_collation c
     JOIN pg_namespace nc ON c.collnamespace = nc.oid) ON d.datcollate = c.collcollate AND d.datctype = c.collctype
  WHERE d.datname = current_database()
  ORDER BY (char_length(c.collname::text)) DESC, c.collname
 LIMIT 1;
;

CREATE VIEW information_schema.check_constraint_routine_usage 
as 
 SELECT DISTINCT current_database()::information_schema.sql_identifier AS constraint_catalog,
    nc.nspname::information_schema.sql_identifier AS constraint_schema,
    c.conname::information_schema.sql_identifier AS constraint_name,
    current_database()::information_schema.sql_identifier AS specific_catalog,
    np.nspname::information_schema.sql_identifier AS specific_schema,
    nameconcatoid(p.proname, p.oid)::information_schema.sql_identifier AS specific_name
   FROM pg_namespace nc,
    pg_constraint c,
    pg_depend d,
    pg_proc p,
    pg_namespace np
  WHERE nc.oid = c.connamespace AND c.contype = 'c'::"char" AND c.oid = d.objid AND d.classid = 'pg_constraint'::regclass::oid AND d.refobjid = p.oid AND d.refclassid = 'pg_proc'::regclass::oid AND p.pronamespace = np.oid AND pg_has_role(p.proowner, 'USAGE'::text);
;

CREATE VIEW information_schema.check_constraints 
as 
 SELECT current_database()::information_schema.sql_identifier AS constraint_catalog,
    rs.nspname::information_schema.sql_identifier AS constraint_schema,
    con.conname::information_schema.sql_identifier AS constraint_name,
    pg_get_expr(con.conbin, COALESCE(c.oid, 0::oid))::information_schema.character_data AS check_clause
   FROM pg_constraint con
     LEFT JOIN pg_namespace rs ON rs.oid = con.connamespace
     LEFT JOIN pg_class c ON c.oid = con.conrelid
     LEFT JOIN pg_type t ON t.oid = con.contypid
  WHERE pg_has_role(COALESCE(c.relowner, t.typowner), 'USAGE'::text) AND con.contype = 'c'::"char"
UNION
 SELECT current_database()::information_schema.sql_identifier AS constraint_catalog,
    rs.nspname::information_schema.sql_identifier AS constraint_schema,
    con.conname::information_schema.sql_identifier AS constraint_name,
    format('%s IS NOT NULL'::text, COALESCE(at.attname, 'VALUE'::name))::information_schema.character_data AS check_clause
   FROM pg_constraint con
     LEFT JOIN pg_namespace rs ON rs.oid = con.connamespace
     LEFT JOIN pg_class c ON c.oid = con.conrelid
     LEFT JOIN pg_type t ON t.oid = con.contypid
     LEFT JOIN pg_attribute at ON con.conrelid = at.attrelid AND con.conkey[1] = at.attnum
  WHERE pg_has_role(COALESCE(c.relowner, t.typowner), 'USAGE'::text) AND con.contype = 'n'::"char"
UNION
 SELECT current_database()::information_schema.sql_identifier AS constraint_catalog,
    n.nspname::information_schema.sql_identifier AS constraint_schema,
    (((((n.oid::text || '_'::text) || r.oid::text) || '_'::text) || a.attnum::text) || '_not_null'::text)::information_schema.sql_identifier AS constraint_name,
    (a.attname::text || ' IS NOT NULL'::text)::information_schema.character_data AS check_clause
   FROM pg_namespace n,
    pg_class r,
    pg_attribute a
  WHERE n.oid = r.relnamespace AND r.oid = a.attrelid AND a.attnum > 0 AND NOT a.attisdropped AND a.attnotnull AND (r.relkind = ANY (ARRAY['r'::"char", 'p'::"char"])) AND pg_has_role(r.relowner, 'USAGE'::text);
;

CREATE VIEW information_schema.collation_character_set_applicability 
as 
 SELECT current_database()::information_schema.sql_identifier AS collation_catalog,
    nc.nspname::information_schema.sql_identifier AS collation_schema,
    c.collname::information_schema.sql_identifier AS collation_name,
    NULL::name::information_schema.sql_identifier AS character_set_catalog,
    NULL::name::information_schema.sql_identifier AS character_set_schema,
    getdatabaseencoding()::information_schema.sql_identifier AS character_set_name
   FROM pg_collation c,
    pg_namespace nc
  WHERE c.collnamespace = nc.oid AND (c.collencoding = ANY (ARRAY['-1'::integer, ( SELECT pg_database.encoding
           FROM pg_database
          WHERE pg_database.datname = current_database())]));
;

CREATE VIEW information_schema.collations 
as 
 SELECT current_database()::information_schema.sql_identifier AS collation_catalog,
    nc.nspname::information_schema.sql_identifier AS collation_schema,
    c.collname::information_schema.sql_identifier AS collation_name,
    'NO PAD'::character varying::information_schema.character_data AS pad_attribute
   FROM pg_collation c,
    pg_namespace nc
  WHERE c.collnamespace = nc.oid AND (c.collencoding = ANY (ARRAY['-1'::integer, ( SELECT pg_database.encoding
           FROM pg_database
          WHERE pg_database.datname = current_database())]));
;

CREATE VIEW information_schema.column_column_usage 
as 
 SELECT DISTINCT current_database()::information_schema.sql_identifier AS table_catalog,
    n.nspname::information_schema.sql_identifier AS table_schema,
    c.relname::information_schema.sql_identifier AS table_name,
    ac.attname::information_schema.sql_identifier AS column_name,
    ad.attname::information_schema.sql_identifier AS dependent_column
   FROM pg_namespace n,
    pg_class c,
    pg_depend d,
    pg_attribute ac,
    pg_attribute ad,
    pg_attrdef atd
  WHERE n.oid = c.relnamespace AND c.oid = ac.attrelid AND c.oid = ad.attrelid AND ac.attnum <> ad.attnum AND ad.attrelid = atd.adrelid AND ad.attnum = atd.adnum AND d.classid = 'pg_attrdef'::regclass::oid AND d.refclassid = 'pg_class'::regclass::oid AND d.objid = atd.oid AND d.refobjid = ac.attrelid AND d.refobjsubid = ac.attnum AND ad.attgenerated <> ''::"char" AND pg_has_role(c.relowner, 'USAGE'::text);
;

CREATE VIEW information_schema.column_domain_usage 
as 
 SELECT current_database()::information_schema.sql_identifier AS domain_catalog,
    nt.nspname::information_schema.sql_identifier AS domain_schema,
    t.typname::information_schema.sql_identifier AS domain_name,
    current_database()::information_schema.sql_identifier AS table_catalog,
    nc.nspname::information_schema.sql_identifier AS table_schema,
    c.relname::information_schema.sql_identifier AS table_name,
    a.attname::information_schema.sql_identifier AS column_name
   FROM pg_type t,
    pg_namespace nt,
    pg_class c,
    pg_namespace nc,
    pg_attribute a
  WHERE t.typnamespace = nt.oid AND c.relnamespace = nc.oid AND a.attrelid = c.oid AND a.atttypid = t.oid AND t.typtype = 'd'::"char" AND (c.relkind = ANY (ARRAY['r'::"char", 'v'::"char", 'f'::"char", 'p'::"char"])) AND a.attnum > 0 AND NOT a.attisdropped AND pg_has_role(t.typowner, 'USAGE'::text);
;

CREATE VIEW information_schema.column_privileges 
as 
 SELECT u_grantor.rolname::information_schema.sql_identifier AS grantor,
    grantee.rolname::information_schema.sql_identifier AS grantee,
    current_database()::information_schema.sql_identifier AS table_catalog,
    nc.nspname::information_schema.sql_identifier AS table_schema,
    x.relname::information_schema.sql_identifier AS table_name,
    x.attname::information_schema.sql_identifier AS column_name,
    x.prtype::information_schema.character_data AS privilege_type,
        CASE
            WHEN pg_has_role(x.grantee, x.relowner, 'USAGE'::text) OR x.grantable THEN 'YES'::text
            ELSE 'NO'::text
        END::information_schema.yes_or_no AS is_grantable
   FROM ( SELECT pr_c.grantor,
            pr_c.grantee,
            a.attname,
            pr_c.relname,
            pr_c.relnamespace,
            pr_c.prtype,
            pr_c.grantable,
            pr_c.relowner
           FROM ( SELECT pg_class.oid,
                    pg_class.relname,
                    pg_class.relnamespace,
                    pg_class.relowner,
                    (aclexplode(COALESCE(pg_class.relacl, acldefault('r'::"char", pg_class.relowner)))).grantor AS grantor,
                    (aclexplode(COALESCE(pg_class.relacl, acldefault('r'::"char", pg_class.relowner)))).grantee AS grantee,
                    (aclexplode(COALESCE(pg_class.relacl, acldefault('r'::"char", pg_class.relowner)))).privilege_type AS privilege_type,
                    (aclexplode(COALESCE(pg_class.relacl, acldefault('r'::"char", pg_class.relowner)))).is_grantable AS is_grantable
                   FROM pg_class
                  WHERE pg_class.relkind = ANY (ARRAY['r'::"char", 'v'::"char", 'f'::"char", 'p'::"char"])) pr_c(oid, relname, relnamespace, relowner, grantor, grantee, prtype, grantable),
            pg_attribute a
          WHERE a.attrelid = pr_c.oid AND a.attnum > 0 AND NOT a.attisdropped
        UNION
         SELECT pr_a.grantor,
            pr_a.grantee,
            pr_a.attname,
            c.relname,
            c.relnamespace,
            pr_a.prtype,
            pr_a.grantable,
            c.relowner
           FROM ( SELECT a.attrelid,
                    a.attname,
                    (aclexplode(COALESCE(a.attacl, acldefault('c'::"char", cc.relowner)))).grantor AS grantor,
                    (aclexplode(COALESCE(a.attacl, acldefault('c'::"char", cc.relowner)))).grantee AS grantee,
                    (aclexplode(COALESCE(a.attacl, acldefault('c'::"char", cc.relowner)))).privilege_type AS privilege_type,
                    (aclexplode(COALESCE(a.attacl, acldefault('c'::"char", cc.relowner)))).is_grantable AS is_grantable
                   FROM pg_attribute a
                     JOIN pg_class cc ON a.attrelid = cc.oid
                  WHERE a.attnum > 0 AND NOT a.attisdropped) pr_a(attrelid, attname, grantor, grantee, prtype, grantable),
            pg_class c
          WHERE pr_a.attrelid = c.oid AND (c.relkind = ANY (ARRAY['r'::"char", 'v'::"char", 'f'::"char", 'p'::"char"]))) x,
    pg_namespace nc,
    pg_authid u_grantor,
    ( SELECT pg_authid.oid,
            pg_authid.rolname
           FROM pg_authid
        UNION ALL
         SELECT 0::oid AS oid,
            'PUBLIC'::name) grantee(oid, rolname)
  WHERE x.relnamespace = nc.oid AND x.grantee = grantee.oid AND x.grantor = u_grantor.oid AND (x.prtype = ANY (ARRAY['INSERT'::text, 'SELECT'::text, 'UPDATE'::text, 'REFERENCES'::text])) AND (pg_has_role(u_grantor.oid, 'USAGE'::text) OR pg_has_role(grantee.oid, 'USAGE'::text) OR grantee.rolname = 'PUBLIC'::name);
;

CREATE VIEW information_schema.column_udt_usage 
as 
 SELECT current_database()::information_schema.sql_identifier AS udt_catalog,
    COALESCE(nbt.nspname, nt.nspname)::information_schema.sql_identifier AS udt_schema,
    COALESCE(bt.typname, t.typname)::information_schema.sql_identifier AS udt_name,
    current_database()::information_schema.sql_identifier AS table_catalog,
    nc.nspname::information_schema.sql_identifier AS table_schema,
    c.relname::information_schema.sql_identifier AS table_name,
    a.attname::information_schema.sql_identifier AS column_name
   FROM pg_attribute a,
    pg_class c,
    pg_namespace nc,
    pg_type t
     JOIN pg_namespace nt ON t.typnamespace = nt.oid
     LEFT JOIN (pg_type bt
     JOIN pg_namespace nbt ON bt.typnamespace = nbt.oid) ON t.typtype = 'd'::"char" AND t.typbasetype = bt.oid
  WHERE a.attrelid = c.oid AND a.atttypid = t.oid AND nc.oid = c.relnamespace AND a.attnum > 0 AND NOT a.attisdropped AND (c.relkind = ANY (ARRAY['r'::"char", 'v'::"char", 'f'::"char", 'p'::"char"])) AND pg_has_role(COALESCE(bt.typowner, t.typowner), 'USAGE'::text);
;

CREATE VIEW information_schema.columns 
as 
 SELECT current_database()::information_schema.sql_identifier AS table_catalog,
    nc.nspname::information_schema.sql_identifier AS table_schema,
    c.relname::information_schema.sql_identifier AS table_name,
    a.attname::information_schema.sql_identifier AS column_name,
    a.attnum::information_schema.cardinal_number AS ordinal_position,
        CASE
            WHEN a.attgenerated = ''::"char" THEN pg_get_expr(ad.adbin, ad.adrelid)
            ELSE NULL::text
        END::information_schema.character_data AS column_default,
        CASE
            WHEN a.attnotnull OR t.typtype = 'd'::"char" AND t.typnotnull THEN 'NO'::text
            ELSE 'YES'::text
        END::information_schema.yes_or_no AS is_nullable,
        CASE
            WHEN t.typtype = 'd'::"char" THEN
            CASE
                WHEN bt.typelem <> 0::oid AND bt.typlen = '-1'::integer THEN 'ARRAY'::text
                WHEN nbt.nspname = 'pg_catalog'::name THEN format_type(t.typbasetype, NULL::integer)
                ELSE 'USER-DEFINED'::text
            END
            ELSE
            CASE
                WHEN t.typelem <> 0::oid AND t.typlen = '-1'::integer THEN 'ARRAY'::text
                WHEN nt.nspname = 'pg_catalog'::name THEN format_type(a.atttypid, NULL::integer)
                ELSE 'USER-DEFINED'::text
            END
        END::information_schema.character_data AS data_type,
    information_schema._pg_char_max_length(information_schema._pg_truetypid(a.*, t.*), information_schema._pg_truetypmod(a.*, t.*))::information_schema.cardinal_number AS character_maximum_length,
    information_schema._pg_char_octet_length(information_schema._pg_truetypid(a.*, t.*), information_schema._pg_truetypmod(a.*, t.*))::information_schema.cardinal_number AS character_octet_length,
    information_schema._pg_numeric_precision(information_schema._pg_truetypid(a.*, t.*), information_schema._pg_truetypmod(a.*, t.*))::information_schema.cardinal_number AS numeric_precision,
    information_schema._pg_numeric_precision_radix(information_schema._pg_truetypid(a.*, t.*), information_schema._pg_truetypmod(a.*, t.*))::information_schema.cardinal_number AS numeric_precision_radix,
    information_schema._pg_numeric_scale(information_schema._pg_truetypid(a.*, t.*), information_schema._pg_truetypmod(a.*, t.*))::information_schema.cardinal_number AS numeric_scale,
    information_schema._pg_datetime_precision(information_schema._pg_truetypid(a.*, t.*), information_schema._pg_truetypmod(a.*, t.*))::information_schema.cardinal_number AS datetime_precision,
    information_schema._pg_interval_type(information_schema._pg_truetypid(a.*, t.*), information_schema._pg_truetypmod(a.*, t.*))::information_schema.character_data AS interval_type,
    NULL::integer::information_schema.cardinal_number AS interval_precision,
    NULL::name::information_schema.sql_identifier AS character_set_catalog,
    NULL::name::information_schema.sql_identifier AS character_set_schema,
    NULL::name::information_schema.sql_identifier AS character_set_name,
        CASE
            WHEN nco.nspname IS NOT NULL THEN current_database()
            ELSE NULL::name
        END::information_schema.sql_identifier AS collation_catalog,
    nco.nspname::information_schema.sql_identifier AS collation_schema,
    co.collname::information_schema.sql_identifier AS collation_name,
        CASE
            WHEN t.typtype = 'd'::"char" THEN current_database()
            ELSE NULL::name
        END::information_schema.sql_identifier AS domain_catalog,
        CASE
            WHEN t.typtype = 'd'::"char" THEN nt.nspname
            ELSE NULL::name
        END::information_schema.sql_identifier AS domain_schema,
        CASE
            WHEN t.typtype = 'd'::"char" THEN t.typname
            ELSE NULL::name
        END::information_schema.sql_identifier AS domain_name,
    current_database()::information_schema.sql_identifier AS udt_catalog,
    COALESCE(nbt.nspname, nt.nspname)::information_schema.sql_identifier AS udt_schema,
    COALESCE(bt.typname, t.typname)::information_schema.sql_identifier AS udt_name,
    NULL::name::information_schema.sql_identifier AS scope_catalog,
    NULL::name::information_schema.sql_identifier AS scope_schema,
    NULL::name::information_schema.sql_identifier AS scope_name,
    NULL::integer::information_schema.cardinal_number AS maximum_cardinality,
    a.attnum::information_schema.sql_identifier AS dtd_identifier,
    'NO'::character varying::information_schema.yes_or_no AS is_self_referencing,
        CASE
            WHEN a.attidentity = ANY (ARRAY['a'::"char", 'd'::"char"]) THEN 'YES'::text
            ELSE 'NO'::text
        END::information_schema.yes_or_no AS is_identity,
        CASE a.attidentity
            WHEN 'a'::"char" THEN 'ALWAYS'::text
            WHEN 'd'::"char" THEN 'BY DEFAULT'::text
            ELSE NULL::text
        END::information_schema.character_data AS identity_generation,
    seq.seqstart::information_schema.character_data AS identity_start,
    seq.seqincrement::information_schema.character_data AS identity_increment,
    seq.seqmax::information_schema.character_data AS identity_maximum,
    seq.seqmin::information_schema.character_data AS identity_minimum,
        CASE
            WHEN seq.seqcycle THEN 'YES'::text
            ELSE 'NO'::text
        END::information_schema.yes_or_no AS identity_cycle,
        CASE
            WHEN a.attgenerated <> ''::"char" THEN 'ALWAYS'::text
            ELSE 'NEVER'::text
        END::information_schema.character_data AS is_generated,
        CASE
            WHEN a.attgenerated <> ''::"char" THEN pg_get_expr(ad.adbin, ad.adrelid)
            ELSE NULL::text
        END::information_schema.character_data AS generation_expression,
        CASE
            WHEN (c.relkind = ANY (ARRAY['r'::"char", 'p'::"char"])) OR (c.relkind = ANY (ARRAY['v'::"char", 'f'::"char"])) AND pg_column_is_updatable(c.oid::regclass, a.attnum, false) THEN 'YES'::text
            ELSE 'NO'::text
        END::information_schema.yes_or_no AS is_updatable
   FROM pg_attribute a
     LEFT JOIN pg_attrdef ad ON a.attrelid = ad.adrelid AND a.attnum = ad.adnum
     JOIN (pg_class c
     JOIN pg_namespace nc ON c.relnamespace = nc.oid) ON a.attrelid = c.oid
     JOIN (pg_type t
     JOIN pg_namespace nt ON t.typnamespace = nt.oid) ON a.atttypid = t.oid
     LEFT JOIN (pg_type bt
     JOIN pg_namespace nbt ON bt.typnamespace = nbt.oid) ON t.typtype = 'd'::"char" AND t.typbasetype = bt.oid
     LEFT JOIN (pg_collation co
     JOIN pg_namespace nco ON co.collnamespace = nco.oid) ON a.attcollation = co.oid AND (nco.nspname <> 'pg_catalog'::name OR co.collname <> 'default'::name)
     LEFT JOIN (pg_depend dep
     JOIN pg_sequence seq ON dep.classid = 'pg_class'::regclass::oid AND dep.objid = seq.seqrelid AND dep.deptype = 'i'::"char") ON dep.refclassid = 'pg_class'::regclass::oid AND dep.refobjid = c.oid AND dep.refobjsubid = a.attnum
  WHERE NOT pg_is_other_temp_schema(nc.oid) AND a.attnum > 0 AND NOT a.attisdropped AND (c.relkind = ANY (ARRAY['r'::"char", 'v'::"char", 'f'::"char", 'p'::"char"])) AND (pg_has_role(c.relowner, 'USAGE'::text) OR has_column_privilege(c.oid, a.attnum, 'SELECT, INSERT, UPDATE, REFERENCES'::text));
;

CREATE VIEW information_schema.constraint_column_usage 
as 
 SELECT current_database()::information_schema.sql_identifier AS table_catalog,
    tblschema::information_schema.sql_identifier AS table_schema,
    tblname::information_schema.sql_identifier AS table_name,
    colname::information_schema.sql_identifier AS column_name,
    current_database()::information_schema.sql_identifier AS constraint_catalog,
    cstrschema::information_schema.sql_identifier AS constraint_schema,
    cstrname::information_schema.sql_identifier AS constraint_name
   FROM ( SELECT DISTINCT nr.nspname,
            r.relname,
            r.relowner,
            a.attname,
            nc.nspname,
            c.conname
           FROM pg_namespace nr,
            pg_class r,
            pg_attribute a,
            pg_depend d,
            pg_namespace nc,
            pg_constraint c
          WHERE nr.oid = r.relnamespace AND r.oid = a.attrelid AND d.refclassid = 'pg_class'::regclass::oid AND d.refobjid = r.oid AND d.refobjsubid = a.attnum AND d.classid = 'pg_constraint'::regclass::oid AND d.objid = c.oid AND c.connamespace = nc.oid AND c.contype = 'c'::"char" AND (r.relkind = ANY (ARRAY['r'::"char", 'p'::"char"])) AND NOT a.attisdropped
        UNION ALL
         SELECT nr.nspname,
            r.relname,
            r.relowner,
            a.attname,
            nc.nspname,
            c.conname
           FROM pg_namespace nr,
            pg_class r,
            pg_attribute a,
            pg_namespace nc,
            pg_constraint c
          WHERE nr.oid = r.relnamespace AND r.oid = a.attrelid AND nc.oid = c.connamespace AND r.oid =
                CASE c.contype
                    WHEN 'f'::"char" THEN c.confrelid
                    ELSE c.conrelid
                END AND (a.attnum = ANY (
                CASE c.contype
                    WHEN 'f'::"char" THEN c.confkey
                    ELSE c.conkey
                END)) AND NOT a.attisdropped AND (c.contype = ANY (ARRAY['p'::"char", 'u'::"char", 'f'::"char"])) AND (r.relkind = ANY (ARRAY['r'::"char", 'p'::"char"]))) x(tblschema, tblname, tblowner, colname, cstrschema, cstrname)
  WHERE pg_has_role(tblowner, 'USAGE'::text);
;

CREATE VIEW information_schema.constraint_table_usage 
as 
 SELECT current_database()::information_schema.sql_identifier AS table_catalog,
    nr.nspname::information_schema.sql_identifier AS table_schema,
    r.relname::information_schema.sql_identifier AS table_name,
    current_database()::information_schema.sql_identifier AS constraint_catalog,
    nc.nspname::information_schema.sql_identifier AS constraint_schema,
    c.conname::information_schema.sql_identifier AS constraint_name
   FROM pg_constraint c,
    pg_namespace nc,
    pg_class r,
    pg_namespace nr
  WHERE c.connamespace = nc.oid AND r.relnamespace = nr.oid AND (c.contype = 'f'::"char" AND c.confrelid = r.oid OR (c.contype = ANY (ARRAY['p'::"char", 'u'::"char"])) AND c.conrelid = r.oid) AND (r.relkind = ANY (ARRAY['r'::"char", 'p'::"char"])) AND pg_has_role(r.relowner, 'USAGE'::text);
;

CREATE VIEW information_schema.domain_constraints 
as 
 SELECT current_database()::information_schema.sql_identifier AS constraint_catalog,
    rs.nspname::information_schema.sql_identifier AS constraint_schema,
    con.conname::information_schema.sql_identifier AS constraint_name,
    current_database()::information_schema.sql_identifier AS domain_catalog,
    n.nspname::information_schema.sql_identifier AS domain_schema,
    t.typname::information_schema.sql_identifier AS domain_name,
        CASE
            WHEN con.condeferrable THEN 'YES'::text
            ELSE 'NO'::text
        END::information_schema.yes_or_no AS is_deferrable,
        CASE
            WHEN con.condeferred THEN 'YES'::text
            ELSE 'NO'::text
        END::information_schema.yes_or_no AS initially_deferred
   FROM pg_namespace rs,
    pg_namespace n,
    pg_constraint con,
    pg_type t
  WHERE rs.oid = con.connamespace AND n.oid = t.typnamespace AND t.oid = con.contypid AND (pg_has_role(t.typowner, 'USAGE'::text) OR has_type_privilege(t.oid, 'USAGE'::text));
;

CREATE VIEW information_schema.domain_udt_usage 
as 
 SELECT current_database()::information_schema.sql_identifier AS udt_catalog,
    nbt.nspname::information_schema.sql_identifier AS udt_schema,
    bt.typname::information_schema.sql_identifier AS udt_name,
    current_database()::information_schema.sql_identifier AS domain_catalog,
    nt.nspname::information_schema.sql_identifier AS domain_schema,
    t.typname::information_schema.sql_identifier AS domain_name
   FROM pg_type t,
    pg_namespace nt,
    pg_type bt,
    pg_namespace nbt
  WHERE t.typnamespace = nt.oid AND t.typbasetype = bt.oid AND bt.typnamespace = nbt.oid AND t.typtype = 'd'::"char" AND pg_has_role(bt.typowner, 'USAGE'::text);
;

CREATE VIEW information_schema.domains 
as 
 SELECT current_database()::information_schema.sql_identifier AS domain_catalog,
    nt.nspname::information_schema.sql_identifier AS domain_schema,
    t.typname::information_schema.sql_identifier AS domain_name,
        CASE
            WHEN t.typelem <> 0::oid AND t.typlen = '-1'::integer THEN 'ARRAY'::text
            WHEN nbt.nspname = 'pg_catalog'::name THEN format_type(t.typbasetype, NULL::integer)
            ELSE 'USER-DEFINED'::text
        END::information_schema.character_data AS data_type,
    information_schema._pg_char_max_length(t.typbasetype, t.typtypmod)::information_schema.cardinal_number AS character_maximum_length,
    information_schema._pg_char_octet_length(t.typbasetype, t.typtypmod)::information_schema.cardinal_number AS character_octet_length,
    NULL::name::information_schema.sql_identifier AS character_set_catalog,
    NULL::name::information_schema.sql_identifier AS character_set_schema,
    NULL::name::information_schema.sql_identifier AS character_set_name,
        CASE
            WHEN nco.nspname IS NOT NULL THEN current_database()
            ELSE NULL::name
        END::information_schema.sql_identifier AS collation_catalog,
    nco.nspname::information_schema.sql_identifier AS collation_schema,
    co.collname::information_schema.sql_identifier AS collation_name,
    information_schema._pg_numeric_precision(t.typbasetype, t.typtypmod)::information_schema.cardinal_number AS numeric_precision,
    information_schema._pg_numeric_precision_radix(t.typbasetype, t.typtypmod)::information_schema.cardinal_number AS numeric_precision_radix,
    information_schema._pg_numeric_scale(t.typbasetype, t.typtypmod)::information_schema.cardinal_number AS numeric_scale,
    information_schema._pg_datetime_precision(t.typbasetype, t.typtypmod)::information_schema.cardinal_number AS datetime_precision,
    information_schema._pg_interval_type(t.typbasetype, t.typtypmod)::information_schema.character_data AS interval_type,
    NULL::integer::information_schema.cardinal_number AS interval_precision,
    t.typdefault::information_schema.character_data AS domain_default,
    current_database()::information_schema.sql_identifier AS udt_catalog,
    nbt.nspname::information_schema.sql_identifier AS udt_schema,
    bt.typname::information_schema.sql_identifier AS udt_name,
    NULL::name::information_schema.sql_identifier AS scope_catalog,
    NULL::name::information_schema.sql_identifier AS scope_schema,
    NULL::name::information_schema.sql_identifier AS scope_name,
    NULL::integer::information_schema.cardinal_number AS maximum_cardinality,
    1::information_schema.sql_identifier AS dtd_identifier
   FROM pg_type t
     JOIN pg_namespace nt ON t.typnamespace = nt.oid
     JOIN (pg_type bt
     JOIN pg_namespace nbt ON bt.typnamespace = nbt.oid) ON t.typbasetype = bt.oid AND t.typtype = 'd'::"char"
     LEFT JOIN (pg_collation co
     JOIN pg_namespace nco ON co.collnamespace = nco.oid) ON t.typcollation = co.oid AND (nco.nspname <> 'pg_catalog'::name OR co.collname <> 'default'::name)
  WHERE pg_has_role(t.typowner, 'USAGE'::text) OR has_type_privilege(t.oid, 'USAGE'::text);
;

CREATE VIEW information_schema.enabled_roles 
as 
 SELECT rolname::information_schema.sql_identifier AS role_name
   FROM pg_authid a
  WHERE pg_has_role(oid, 'USAGE'::text);
;

CREATE VIEW information_schema.information_schema_catalog_name 
as 
 SELECT current_database()::information_schema.sql_identifier AS catalog_name;
;

CREATE VIEW information_schema.key_column_usage 
as 
 SELECT current_database()::information_schema.sql_identifier AS constraint_catalog,
    ss.nc_nspname::information_schema.sql_identifier AS constraint_schema,
    ss.conname::information_schema.sql_identifier AS constraint_name,
    current_database()::information_schema.sql_identifier AS table_catalog,
    ss.nr_nspname::information_schema.sql_identifier AS table_schema,
    ss.relname::information_schema.sql_identifier AS table_name,
    a.attname::information_schema.sql_identifier AS column_name,
    (ss.x).n::information_schema.cardinal_number AS ordinal_position,
        CASE
            WHEN ss.contype = 'f'::"char" THEN information_schema._pg_index_position(ss.conindid, ss.confkey[(ss.x).n])
            ELSE NULL::integer
        END::information_schema.cardinal_number AS position_in_unique_constraint
   FROM pg_attribute a,
    ( SELECT r.oid AS roid,
            r.relname,
            r.relowner,
            nc.nspname AS nc_nspname,
            nr.nspname AS nr_nspname,
            c.oid AS coid,
            c.conname,
            c.contype,
            c.conindid,
            c.confkey,
            c.confrelid,
            information_schema._pg_expandarray(c.conkey) AS x
           FROM pg_namespace nr,
            pg_class r,
            pg_namespace nc,
            pg_constraint c
          WHERE nr.oid = r.relnamespace AND r.oid = c.conrelid AND nc.oid = c.connamespace AND (c.contype = ANY (ARRAY['p'::"char", 'u'::"char", 'f'::"char"])) AND (r.relkind = ANY (ARRAY['r'::"char", 'p'::"char"])) AND NOT pg_is_other_temp_schema(nr.oid)) ss
  WHERE ss.roid = a.attrelid AND a.attnum = (ss.x).x AND NOT a.attisdropped AND (pg_has_role(ss.relowner, 'USAGE'::text) OR has_column_privilege(ss.roid, a.attnum, 'SELECT, INSERT, UPDATE, REFERENCES'::text));
;

CREATE VIEW information_schema.parameters 
as 
 SELECT current_database()::information_schema.sql_identifier AS specific_catalog,
    ss.n_nspname::information_schema.sql_identifier AS specific_schema,
    nameconcatoid(ss.proname, ss.p_oid)::information_schema.sql_identifier AS specific_name,
    (ss.x).n::information_schema.cardinal_number AS ordinal_position,
        CASE
            WHEN ss.proargmodes IS NULL THEN 'IN'::text
            WHEN ss.proargmodes[(ss.x).n] = 'i'::"char" THEN 'IN'::text
            WHEN ss.proargmodes[(ss.x).n] = 'o'::"char" THEN 'OUT'::text
            WHEN ss.proargmodes[(ss.x).n] = 'b'::"char" THEN 'INOUT'::text
            WHEN ss.proargmodes[(ss.x).n] = 'v'::"char" THEN 'IN'::text
            WHEN ss.proargmodes[(ss.x).n] = 't'::"char" THEN 'OUT'::text
            ELSE NULL::text
        END::information_schema.character_data AS parameter_mode,
    'NO'::character varying::information_schema.yes_or_no AS is_result,
    'NO'::character varying::information_schema.yes_or_no AS as_locator,
    NULLIF(ss.proargnames[(ss.x).n], ''::text)::information_schema.sql_identifier AS parameter_name,
        CASE
            WHEN t.typelem <> 0::oid AND t.typlen = '-1'::integer THEN 'ARRAY'::text
            WHEN nt.nspname = 'pg_catalog'::name THEN format_type(t.oid, NULL::integer)
            ELSE 'USER-DEFINED'::text
        END::information_schema.character_data AS data_type,
    NULL::integer::information_schema.cardinal_number AS character_maximum_length,
    NULL::integer::information_schema.cardinal_number AS character_octet_length,
    NULL::name::information_schema.sql_identifier AS character_set_catalog,
    NULL::name::information_schema.sql_identifier AS character_set_schema,
    NULL::name::information_schema.sql_identifier AS character_set_name,
    NULL::name::information_schema.sql_identifier AS collation_catalog,
    NULL::name::information_schema.sql_identifier AS collation_schema,
    NULL::name::information_schema.sql_identifier AS collation_name,
    NULL::integer::information_schema.cardinal_number AS numeric_precision,
    NULL::integer::information_schema.cardinal_number AS numeric_precision_radix,
    NULL::integer::information_schema.cardinal_number AS numeric_scale,
    NULL::integer::information_schema.cardinal_number AS datetime_precision,
    NULL::character varying::information_schema.character_data AS interval_type,
    NULL::integer::information_schema.cardinal_number AS interval_precision,
    current_database()::information_schema.sql_identifier AS udt_catalog,
    nt.nspname::information_schema.sql_identifier AS udt_schema,
    t.typname::information_schema.sql_identifier AS udt_name,
    NULL::name::information_schema.sql_identifier AS scope_catalog,
    NULL::name::information_schema.sql_identifier AS scope_schema,
    NULL::name::information_schema.sql_identifier AS scope_name,
    NULL::integer::information_schema.cardinal_number AS maximum_cardinality,
    (ss.x).n::information_schema.sql_identifier AS dtd_identifier,
        CASE
            WHEN pg_has_role(ss.proowner, 'USAGE'::text) THEN pg_get_function_arg_default(ss.p_oid, (ss.x).n)
            ELSE NULL::text
        END::information_schema.character_data AS parameter_default
   FROM pg_type t,
    pg_namespace nt,
    ( SELECT n.nspname AS n_nspname,
            p.proname,
            p.oid AS p_oid,
            p.proowner,
            p.proargnames,
            p.proargmodes,
            information_schema._pg_expandarray(COALESCE(p.proallargtypes, p.proargtypes::oid[])) AS x
           FROM pg_namespace n,
            pg_proc p
          WHERE n.oid = p.pronamespace AND (pg_has_role(p.proowner, 'USAGE'::text) OR has_function_privilege(p.oid, 'EXECUTE'::text))) ss
  WHERE t.oid = (ss.x).x AND t.typnamespace = nt.oid;
;

CREATE VIEW information_schema.referential_constraints 
as 
 SELECT current_database()::information_schema.sql_identifier AS constraint_catalog,
    ncon.nspname::information_schema.sql_identifier AS constraint_schema,
    con.conname::information_schema.sql_identifier AS constraint_name,
        CASE
            WHEN npkc.nspname IS NULL THEN NULL::name
            ELSE current_database()
        END::information_schema.sql_identifier AS unique_constraint_catalog,
    npkc.nspname::information_schema.sql_identifier AS unique_constraint_schema,
    pkc.conname::information_schema.sql_identifier AS unique_constraint_name,
        CASE con.confmatchtype
            WHEN 'f'::"char" THEN 'FULL'::text
            WHEN 'p'::"char" THEN 'PARTIAL'::text
            WHEN 's'::"char" THEN 'NONE'::text
            ELSE NULL::text
        END::information_schema.character_data AS match_option,
        CASE con.confupdtype
            WHEN 'c'::"char" THEN 'CASCADE'::text
            WHEN 'n'::"char" THEN 'SET NULL'::text
            WHEN 'd'::"char" THEN 'SET DEFAULT'::text
            WHEN 'r'::"char" THEN 'RESTRICT'::text
            WHEN 'a'::"char" THEN 'NO ACTION'::text
            ELSE NULL::text
        END::information_schema.character_data AS update_rule,
        CASE con.confdeltype
            WHEN 'c'::"char" THEN 'CASCADE'::text
            WHEN 'n'::"char" THEN 'SET NULL'::text
            WHEN 'd'::"char" THEN 'SET DEFAULT'::text
            WHEN 'r'::"char" THEN 'RESTRICT'::text
            WHEN 'a'::"char" THEN 'NO ACTION'::text
            ELSE NULL::text
        END::information_schema.character_data AS delete_rule
   FROM pg_namespace ncon
     JOIN pg_constraint con ON ncon.oid = con.connamespace
     JOIN pg_class c ON con.conrelid = c.oid AND con.contype = 'f'::"char"
     LEFT JOIN pg_depend d1 ON d1.objid = con.oid AND d1.classid = 'pg_constraint'::regclass::oid AND d1.refclassid = 'pg_class'::regclass::oid AND d1.refobjsubid = 0
     LEFT JOIN pg_depend d2 ON d2.refclassid = 'pg_constraint'::regclass::oid AND d2.classid = 'pg_class'::regclass::oid AND d2.objid = d1.refobjid AND d2.objsubid = 0 AND d2.deptype = 'i'::"char"
     LEFT JOIN pg_constraint pkc ON pkc.oid = d2.refobjid AND (pkc.contype = ANY (ARRAY['p'::"char", 'u'::"char"])) AND pkc.conrelid = con.confrelid
     LEFT JOIN pg_namespace npkc ON pkc.connamespace = npkc.oid
  WHERE pg_has_role(c.relowner, 'USAGE'::text) OR has_table_privilege(c.oid, 'INSERT, UPDATE, DELETE, TRUNCATE, REFERENCES, TRIGGER'::text) OR has_any_column_privilege(c.oid, 'INSERT, UPDATE, REFERENCES'::text);
;

CREATE VIEW information_schema.routine_column_usage 
as 
 SELECT DISTINCT current_database()::information_schema.sql_identifier AS specific_catalog,
    np.nspname::information_schema.sql_identifier AS specific_schema,
    nameconcatoid(p.proname, p.oid)::information_schema.sql_identifier AS specific_name,
    current_database()::information_schema.sql_identifier AS routine_catalog,
    np.nspname::information_schema.sql_identifier AS routine_schema,
    p.proname::information_schema.sql_identifier AS routine_name,
    current_database()::information_schema.sql_identifier AS table_catalog,
    nt.nspname::information_schema.sql_identifier AS table_schema,
    t.relname::information_schema.sql_identifier AS table_name,
    a.attname::information_schema.sql_identifier AS column_name
   FROM pg_namespace np,
    pg_proc p,
    pg_depend d,
    pg_class t,
    pg_namespace nt,
    pg_attribute a
  WHERE np.oid = p.pronamespace AND p.oid = d.objid AND d.classid = 'pg_proc'::regclass::oid AND d.refobjid = t.oid AND d.refclassid = 'pg_class'::regclass::oid AND t.relnamespace = nt.oid AND (t.relkind = ANY (ARRAY['r'::"char", 'v'::"char", 'f'::"char", 'p'::"char"])) AND t.oid = a.attrelid AND d.refobjsubid = a.attnum AND pg_has_role(t.relowner, 'USAGE'::text);
;

CREATE VIEW information_schema.routine_privileges 
as 
 SELECT u_grantor.rolname::information_schema.sql_identifier AS grantor,
    grantee.rolname::information_schema.sql_identifier AS grantee,
    current_database()::information_schema.sql_identifier AS specific_catalog,
    n.nspname::information_schema.sql_identifier AS specific_schema,
    nameconcatoid(p.proname, p.oid)::information_schema.sql_identifier AS specific_name,
    current_database()::information_schema.sql_identifier AS routine_catalog,
    n.nspname::information_schema.sql_identifier AS routine_schema,
    p.proname::information_schema.sql_identifier AS routine_name,
    'EXECUTE'::character varying::information_schema.character_data AS privilege_type,
        CASE
            WHEN pg_has_role(grantee.oid, p.proowner, 'USAGE'::text) OR p.grantable THEN 'YES'::text
            ELSE 'NO'::text
        END::information_schema.yes_or_no AS is_grantable
   FROM ( SELECT pg_proc.oid,
            pg_proc.proname,
            pg_proc.proowner,
            pg_proc.pronamespace,
            (aclexplode(COALESCE(pg_proc.proacl, acldefault('f'::"char", pg_proc.proowner)))).grantor AS grantor,
            (aclexplode(COALESCE(pg_proc.proacl, acldefault('f'::"char", pg_proc.proowner)))).grantee AS grantee,
            (aclexplode(COALESCE(pg_proc.proacl, acldefault('f'::"char", pg_proc.proowner)))).privilege_type AS privilege_type,
            (aclexplode(COALESCE(pg_proc.proacl, acldefault('f'::"char", pg_proc.proowner)))).is_grantable AS is_grantable
           FROM pg_proc) p(oid, proname, proowner, pronamespace, grantor, grantee, prtype, grantable),
    pg_namespace n,
    pg_authid u_grantor,
    ( SELECT pg_authid.oid,
            pg_authid.rolname
           FROM pg_authid
        UNION ALL
         SELECT 0::oid AS oid,
            'PUBLIC'::name) grantee(oid, rolname)
  WHERE p.pronamespace = n.oid AND grantee.oid = p.grantee AND u_grantor.oid = p.grantor AND p.prtype = 'EXECUTE'::text AND (pg_has_role(u_grantor.oid, 'USAGE'::text) OR pg_has_role(grantee.oid, 'USAGE'::text) OR grantee.rolname = 'PUBLIC'::name);
;

CREATE VIEW information_schema.routine_routine_usage 
as 
 SELECT DISTINCT current_database()::information_schema.sql_identifier AS specific_catalog,
    np.nspname::information_schema.sql_identifier AS specific_schema,
    nameconcatoid(p.proname, p.oid)::information_schema.sql_identifier AS specific_name,
    current_database()::information_schema.sql_identifier AS routine_catalog,
    np1.nspname::information_schema.sql_identifier AS routine_schema,
    nameconcatoid(p1.proname, p1.oid)::information_schema.sql_identifier AS routine_name
   FROM pg_namespace np,
    pg_proc p,
    pg_depend d,
    pg_proc p1,
    pg_namespace np1
  WHERE np.oid = p.pronamespace AND p.oid = d.objid AND d.classid = 'pg_proc'::regclass::oid AND d.refobjid = p1.oid AND d.refclassid = 'pg_proc'::regclass::oid AND p1.pronamespace = np1.oid AND (p.prokind = ANY (ARRAY['f'::"char", 'p'::"char"])) AND (p1.prokind = ANY (ARRAY['f'::"char", 'p'::"char"])) AND pg_has_role(p1.proowner, 'USAGE'::text);
;

CREATE VIEW information_schema.routine_sequence_usage 
as 
 SELECT DISTINCT current_database()::information_schema.sql_identifier AS specific_catalog,
    np.nspname::information_schema.sql_identifier AS specific_schema,
    nameconcatoid(p.proname, p.oid)::information_schema.sql_identifier AS specific_name,
    current_database()::information_schema.sql_identifier AS routine_catalog,
    np.nspname::information_schema.sql_identifier AS routine_schema,
    p.proname::information_schema.sql_identifier AS routine_name,
    current_database()::information_schema.sql_identifier AS sequence_catalog,
    ns.nspname::information_schema.sql_identifier AS sequence_schema,
    s.relname::information_schema.sql_identifier AS sequence_name
   FROM pg_namespace np,
    pg_proc p,
    pg_depend d,
    pg_class s,
    pg_namespace ns
  WHERE np.oid = p.pronamespace AND p.oid = d.objid AND d.classid = 'pg_proc'::regclass::oid AND d.refobjid = s.oid AND d.refclassid = 'pg_class'::regclass::oid AND s.relnamespace = ns.oid AND s.relkind = 'S'::"char" AND pg_has_role(s.relowner, 'USAGE'::text);
;

CREATE VIEW information_schema.routine_table_usage 
as 
 SELECT DISTINCT current_database()::information_schema.sql_identifier AS specific_catalog,
    np.nspname::information_schema.sql_identifier AS specific_schema,
    nameconcatoid(p.proname, p.oid)::information_schema.sql_identifier AS specific_name,
    current_database()::information_schema.sql_identifier AS routine_catalog,
    np.nspname::information_schema.sql_identifier AS routine_schema,
    p.proname::information_schema.sql_identifier AS routine_name,
    current_database()::information_schema.sql_identifier AS table_catalog,
    nt.nspname::information_schema.sql_identifier AS table_schema,
    t.relname::information_schema.sql_identifier AS table_name
   FROM pg_namespace np,
    pg_proc p,
    pg_depend d,
    pg_class t,
    pg_namespace nt
  WHERE np.oid = p.pronamespace AND p.oid = d.objid AND d.classid = 'pg_proc'::regclass::oid AND d.refobjid = t.oid AND d.refclassid = 'pg_class'::regclass::oid AND t.relnamespace = nt.oid AND (t.relkind = ANY (ARRAY['r'::"char", 'v'::"char", 'f'::"char", 'p'::"char"])) AND pg_has_role(t.relowner, 'USAGE'::text);
;

CREATE VIEW information_schema.routines 
as 
 SELECT current_database()::information_schema.sql_identifier AS specific_catalog,
    n.nspname::information_schema.sql_identifier AS specific_schema,
    nameconcatoid(p.proname, p.oid)::information_schema.sql_identifier AS specific_name,
    current_database()::information_schema.sql_identifier AS routine_catalog,
    n.nspname::information_schema.sql_identifier AS routine_schema,
    p.proname::information_schema.sql_identifier AS routine_name,
        CASE p.prokind
            WHEN 'f'::"char" THEN 'FUNCTION'::text
            WHEN 'p'::"char" THEN 'PROCEDURE'::text
            ELSE NULL::text
        END::information_schema.character_data AS routine_type,
    NULL::name::information_schema.sql_identifier AS module_catalog,
    NULL::name::information_schema.sql_identifier AS module_schema,
    NULL::name::information_schema.sql_identifier AS module_name,
    NULL::name::information_schema.sql_identifier AS udt_catalog,
    NULL::name::information_schema.sql_identifier AS udt_schema,
    NULL::name::information_schema.sql_identifier AS udt_name,
        CASE
            WHEN p.prokind = 'p'::"char" THEN NULL::text
            WHEN t.typelem <> 0::oid AND t.typlen = '-1'::integer THEN 'ARRAY'::text
            WHEN nt.nspname = 'pg_catalog'::name THEN format_type(t.oid, NULL::integer)
            ELSE 'USER-DEFINED'::text
        END::information_schema.character_data AS data_type,
    NULL::integer::information_schema.cardinal_number AS character_maximum_length,
    NULL::integer::information_schema.cardinal_number AS character_octet_length,
    NULL::name::information_schema.sql_identifier AS character_set_catalog,
    NULL::name::information_schema.sql_identifier AS character_set_schema,
    NULL::name::information_schema.sql_identifier AS character_set_name,
    NULL::name::information_schema.sql_identifier AS collation_catalog,
    NULL::name::information_schema.sql_identifier AS collation_schema,
    NULL::name::information_schema.sql_identifier AS collation_name,
    NULL::integer::information_schema.cardinal_number AS numeric_precision,
    NULL::integer::information_schema.cardinal_number AS numeric_precision_radix,
    NULL::integer::information_schema.cardinal_number AS numeric_scale,
    NULL::integer::information_schema.cardinal_number AS datetime_precision,
    NULL::character varying::information_schema.character_data AS interval_type,
    NULL::integer::information_schema.cardinal_number AS interval_precision,
        CASE
            WHEN nt.nspname IS NOT NULL THEN current_database()
            ELSE NULL::name
        END::information_schema.sql_identifier AS type_udt_catalog,
    nt.nspname::information_schema.sql_identifier AS type_udt_schema,
    t.typname::information_schema.sql_identifier AS type_udt_name,
    NULL::name::information_schema.sql_identifier AS scope_catalog,
    NULL::name::information_schema.sql_identifier AS scope_schema,
    NULL::name::information_schema.sql_identifier AS scope_name,
    NULL::integer::information_schema.cardinal_number AS maximum_cardinality,
        CASE
            WHEN p.prokind <> 'p'::"char" THEN 0
            ELSE NULL::integer
        END::information_schema.sql_identifier AS dtd_identifier,
        CASE
            WHEN l.lanname = 'sql'::name THEN 'SQL'::text
            ELSE 'EXTERNAL'::text
        END::information_schema.character_data AS routine_body,
        CASE
            WHEN pg_has_role(p.proowner, 'USAGE'::text) THEN p.prosrc
            ELSE NULL::text
        END::information_schema.character_data AS routine_definition,
        CASE
            WHEN l.lanname = 'c'::name THEN p.prosrc
            ELSE NULL::text
        END::information_schema.character_data AS external_name,
    upper(l.lanname::text)::information_schema.character_data AS external_language,
    'GENERAL'::character varying::information_schema.character_data AS parameter_style,
        CASE
            WHEN p.provolatile = 'i'::"char" THEN 'YES'::text
            ELSE 'NO'::text
        END::information_schema.yes_or_no AS is_deterministic,
    'MODIFIES'::character varying::information_schema.character_data AS sql_data_access,
        CASE
            WHEN p.prokind <> 'p'::"char" THEN
            CASE
                WHEN p.proisstrict THEN 'YES'::text
                ELSE 'NO'::text
            END
            ELSE NULL::text
        END::information_schema.yes_or_no AS is_null_call,
    NULL::character varying::information_schema.character_data AS sql_path,
    'YES'::character varying::information_schema.yes_or_no AS schema_level_routine,
    0::information_schema.cardinal_number AS max_dynamic_result_sets,
    NULL::character varying::information_schema.yes_or_no AS is_user_defined_cast,
    NULL::character varying::information_schema.yes_or_no AS is_implicitly_invocable,
        CASE
            WHEN p.prosecdef THEN 'DEFINER'::text
            ELSE 'INVOKER'::text
        END::information_schema.character_data AS security_type,
    NULL::name::information_schema.sql_identifier AS to_sql_specific_catalog,
    NULL::name::information_schema.sql_identifier AS to_sql_specific_schema,
    NULL::name::information_schema.sql_identifier AS to_sql_specific_name,
    'NO'::character varying::information_schema.yes_or_no AS as_locator,
    NULL::timestamp with time zone::information_schema.time_stamp AS created,
    NULL::timestamp with time zone::information_schema.time_stamp AS last_altered,
    NULL::character varying::information_schema.yes_or_no AS new_savepoint_level,
    'NO'::character varying::information_schema.yes_or_no AS is_udt_dependent,
    NULL::character varying::information_schema.character_data AS result_cast_from_data_type,
    NULL::character varying::information_schema.yes_or_no AS result_cast_as_locator,
    NULL::integer::information_schema.cardinal_number AS result_cast_char_max_length,
    NULL::integer::information_schema.cardinal_number AS result_cast_char_octet_length,
    NULL::name::information_schema.sql_identifier AS result_cast_char_set_catalog,
    NULL::name::information_schema.sql_identifier AS result_cast_char_set_schema,
    NULL::name::information_schema.sql_identifier AS result_cast_char_set_name,
    NULL::name::information_schema.sql_identifier AS result_cast_collation_catalog,
    NULL::name::information_schema.sql_identifier AS result_cast_collation_schema,
    NULL::name::information_schema.sql_identifier AS result_cast_collation_name,
    NULL::integer::information_schema.cardinal_number AS result_cast_numeric_precision,
    NULL::integer::information_schema.cardinal_number AS result_cast_numeric_precision_radix,
    NULL::integer::information_schema.cardinal_number AS result_cast_numeric_scale,
    NULL::integer::information_schema.cardinal_number AS result_cast_datetime_precision,
    NULL::character varying::information_schema.character_data AS result_cast_interval_type,
    NULL::integer::information_schema.cardinal_number AS result_cast_interval_precision,
    NULL::name::information_schema.sql_identifier AS result_cast_type_udt_catalog,
    NULL::name::information_schema.sql_identifier AS result_cast_type_udt_schema,
    NULL::name::information_schema.sql_identifier AS result_cast_type_udt_name,
    NULL::name::information_schema.sql_identifier AS result_cast_scope_catalog,
    NULL::name::information_schema.sql_identifier AS result_cast_scope_schema,
    NULL::name::information_schema.sql_identifier AS result_cast_scope_name,
    NULL::integer::information_schema.cardinal_number AS result_cast_maximum_cardinality,
    NULL::name::information_schema.sql_identifier AS result_cast_dtd_identifier
   FROM pg_namespace n
     JOIN pg_proc p ON n.oid = p.pronamespace
     JOIN pg_language l ON p.prolang = l.oid
     LEFT JOIN (pg_type t
     JOIN pg_namespace nt ON t.typnamespace = nt.oid) ON p.prorettype = t.oid AND p.prokind <> 'p'::"char"
  WHERE pg_has_role(p.proowner, 'USAGE'::text) OR has_function_privilege(p.oid, 'EXECUTE'::text);
;

CREATE VIEW information_schema.schemata 
as 
 SELECT current_database()::information_schema.sql_identifier AS catalog_name,
    n.nspname::information_schema.sql_identifier AS schema_name,
    u.rolname::information_schema.sql_identifier AS schema_owner,
    NULL::name::information_schema.sql_identifier AS default_character_set_catalog,
    NULL::name::information_schema.sql_identifier AS default_character_set_schema,
    NULL::name::information_schema.sql_identifier AS default_character_set_name,
    NULL::character varying::information_schema.character_data AS sql_path
   FROM pg_namespace n,
    pg_authid u
  WHERE n.nspowner = u.oid AND (pg_has_role(n.nspowner, 'USAGE'::text) OR has_schema_privilege(n.oid, 'CREATE, USAGE'::text));
;

CREATE VIEW information_schema.sequences 
as 
 SELECT current_database()::information_schema.sql_identifier AS sequence_catalog,
    nc.nspname::information_schema.sql_identifier AS sequence_schema,
    c.relname::information_schema.sql_identifier AS sequence_name,
    format_type(s.seqtypid, NULL::integer)::information_schema.character_data AS data_type,
    information_schema._pg_numeric_precision(s.seqtypid, '-1'::integer)::information_schema.cardinal_number AS numeric_precision,
    2::information_schema.cardinal_number AS numeric_precision_radix,
    0::information_schema.cardinal_number AS numeric_scale,
    s.seqstart::information_schema.character_data AS start_value,
    s.seqmin::information_schema.character_data AS minimum_value,
    s.seqmax::information_schema.character_data AS maximum_value,
    s.seqincrement::information_schema.character_data AS increment,
        CASE
            WHEN s.seqcycle THEN 'YES'::text
            ELSE 'NO'::text
        END::information_schema.yes_or_no AS cycle_option
   FROM pg_namespace nc,
    pg_class c,
    pg_sequence s
  WHERE c.relnamespace = nc.oid AND c.relkind = 'S'::"char" AND NOT (EXISTS ( SELECT 1
           FROM pg_depend
          WHERE pg_depend.classid = 'pg_class'::regclass::oid AND pg_depend.objid = c.oid AND pg_depend.deptype = 'i'::"char")) AND NOT pg_is_other_temp_schema(nc.oid) AND c.oid = s.seqrelid AND (pg_has_role(c.relowner, 'USAGE'::text) OR has_sequence_privilege(c.oid, 'SELECT, UPDATE, USAGE'::text));
;

CREATE VIEW information_schema.table_constraints 
as 
 SELECT current_database()::information_schema.sql_identifier AS constraint_catalog,
    nc.nspname::information_schema.sql_identifier AS constraint_schema,
    c.conname::information_schema.sql_identifier AS constraint_name,
    current_database()::information_schema.sql_identifier AS table_catalog,
    nr.nspname::information_schema.sql_identifier AS table_schema,
    r.relname::information_schema.sql_identifier AS table_name,
        CASE c.contype
            WHEN 'c'::"char" THEN 'CHECK'::text
            WHEN 'f'::"char" THEN 'FOREIGN KEY'::text
            WHEN 'p'::"char" THEN 'PRIMARY KEY'::text
            WHEN 'u'::"char" THEN 'UNIQUE'::text
            ELSE NULL::text
        END::information_schema.character_data AS constraint_type,
        CASE
            WHEN c.condeferrable THEN 'YES'::text
            ELSE 'NO'::text
        END::information_schema.yes_or_no AS is_deferrable,
        CASE
            WHEN c.condeferred THEN 'YES'::text
            ELSE 'NO'::text
        END::information_schema.yes_or_no AS initially_deferred,
    'YES'::character varying::information_schema.yes_or_no AS enforced,
        CASE
            WHEN c.contype = 'u'::"char" THEN
            CASE
                WHEN ( SELECT NOT pg_index.indnullsnotdistinct
                   FROM pg_index
                  WHERE pg_index.indexrelid = c.conindid) THEN 'YES'::text
                ELSE 'NO'::text
            END
            ELSE NULL::text
        END::information_schema.yes_or_no AS nulls_distinct
   FROM pg_namespace nc,
    pg_namespace nr,
    pg_constraint c,
    pg_class r
  WHERE nc.oid = c.connamespace AND nr.oid = r.relnamespace AND c.conrelid = r.oid AND (c.contype <> ALL (ARRAY['t'::"char", 'x'::"char"])) AND (r.relkind = ANY (ARRAY['r'::"char", 'p'::"char"])) AND NOT pg_is_other_temp_schema(nr.oid) AND (pg_has_role(r.relowner, 'USAGE'::text) OR has_table_privilege(r.oid, 'INSERT, UPDATE, DELETE, TRUNCATE, REFERENCES, TRIGGER'::text) OR has_any_column_privilege(r.oid, 'INSERT, UPDATE, REFERENCES'::text))
UNION ALL
 SELECT current_database()::information_schema.sql_identifier AS constraint_catalog,
    nr.nspname::information_schema.sql_identifier AS constraint_schema,
    (((((nr.oid::text || '_'::text) || r.oid::text) || '_'::text) || a.attnum::text) || '_not_null'::text)::information_schema.sql_identifier AS constraint_name,
    current_database()::information_schema.sql_identifier AS table_catalog,
    nr.nspname::information_schema.sql_identifier AS table_schema,
    r.relname::information_schema.sql_identifier AS table_name,
    'CHECK'::character varying::information_schema.character_data AS constraint_type,
    'NO'::character varying::information_schema.yes_or_no AS is_deferrable,
    'NO'::character varying::information_schema.yes_or_no AS initially_deferred,
    'YES'::character varying::information_schema.yes_or_no AS enforced,
    NULL::character varying::information_schema.yes_or_no AS nulls_distinct
   FROM pg_namespace nr,
    pg_class r,
    pg_attribute a
  WHERE nr.oid = r.relnamespace AND r.oid = a.attrelid AND a.attnotnull AND a.attnum > 0 AND NOT a.attisdropped AND (r.relkind = ANY (ARRAY['r'::"char", 'p'::"char"])) AND NOT pg_is_other_temp_schema(nr.oid) AND (pg_has_role(r.relowner, 'USAGE'::text) OR has_table_privilege(r.oid, 'INSERT, UPDATE, DELETE, TRUNCATE, REFERENCES, TRIGGER'::text) OR has_any_column_privilege(r.oid, 'INSERT, UPDATE, REFERENCES'::text));
;

CREATE VIEW information_schema.table_privileges 
as 
 SELECT u_grantor.rolname::information_schema.sql_identifier AS grantor,
    grantee.rolname::information_schema.sql_identifier AS grantee,
    current_database()::information_schema.sql_identifier AS table_catalog,
    nc.nspname::information_schema.sql_identifier AS table_schema,
    c.relname::information_schema.sql_identifier AS table_name,
    c.prtype::information_schema.character_data AS privilege_type,
        CASE
            WHEN pg_has_role(grantee.oid, c.relowner, 'USAGE'::text) OR c.grantable THEN 'YES'::text
            ELSE 'NO'::text
        END::information_schema.yes_or_no AS is_grantable,
        CASE
            WHEN c.prtype = 'SELECT'::text THEN 'YES'::text
            ELSE 'NO'::text
        END::information_schema.yes_or_no AS with_hierarchy
   FROM ( SELECT pg_class.oid,
            pg_class.relname,
            pg_class.relnamespace,
            pg_class.relkind,
            pg_class.relowner,
            (aclexplode(COALESCE(pg_class.relacl, acldefault('r'::"char", pg_class.relowner)))).grantor AS grantor,
            (aclexplode(COALESCE(pg_class.relacl, acldefault('r'::"char", pg_class.relowner)))).grantee AS grantee,
            (aclexplode(COALESCE(pg_class.relacl, acldefault('r'::"char", pg_class.relowner)))).privilege_type AS privilege_type,
            (aclexplode(COALESCE(pg_class.relacl, acldefault('r'::"char", pg_class.relowner)))).is_grantable AS is_grantable
           FROM pg_class) c(oid, relname, relnamespace, relkind, relowner, grantor, grantee, prtype, grantable),
    pg_namespace nc,
    pg_authid u_grantor,
    ( SELECT pg_authid.oid,
            pg_authid.rolname
           FROM pg_authid
        UNION ALL
         SELECT 0::oid AS oid,
            'PUBLIC'::name) grantee(oid, rolname)
  WHERE c.relnamespace = nc.oid AND (c.relkind = ANY (ARRAY['r'::"char", 'v'::"char", 'f'::"char", 'p'::"char"])) AND c.grantee = grantee.oid AND c.grantor = u_grantor.oid AND (c.prtype = ANY (ARRAY['INSERT'::text, 'SELECT'::text, 'UPDATE'::text, 'DELETE'::text, 'TRUNCATE'::text, 'REFERENCES'::text, 'TRIGGER'::text])) AND (pg_has_role(u_grantor.oid, 'USAGE'::text) OR pg_has_role(grantee.oid, 'USAGE'::text) OR grantee.rolname = 'PUBLIC'::name);
;

CREATE VIEW information_schema.tables 
as 
 SELECT current_database()::information_schema.sql_identifier AS table_catalog,
    nc.nspname::information_schema.sql_identifier AS table_schema,
    c.relname::information_schema.sql_identifier AS table_name,
        CASE
            WHEN nc.oid = pg_my_temp_schema() THEN 'LOCAL TEMPORARY'::text
            WHEN c.relkind = ANY (ARRAY['r'::"char", 'p'::"char"]) THEN 'BASE TABLE'::text
            WHEN c.relkind = 'v'::"char" THEN 'VIEW'::text
            WHEN c.relkind = 'f'::"char" THEN 'FOREIGN'::text
            ELSE NULL::text
        END::information_schema.character_data AS table_type,
    NULL::name::information_schema.sql_identifier AS self_referencing_column_name,
    NULL::character varying::information_schema.character_data AS reference_generation,
        CASE
            WHEN t.typname IS NOT NULL THEN current_database()
            ELSE NULL::name
        END::information_schema.sql_identifier AS user_defined_type_catalog,
    nt.nspname::information_schema.sql_identifier AS user_defined_type_schema,
    t.typname::information_schema.sql_identifier AS user_defined_type_name,
        CASE
            WHEN (c.relkind = ANY (ARRAY['r'::"char", 'p'::"char"])) OR (c.relkind = ANY (ARRAY['v'::"char", 'f'::"char"])) AND (pg_relation_is_updatable(c.oid::regclass, false) & 8) = 8 THEN 'YES'::text
            ELSE 'NO'::text
        END::information_schema.yes_or_no AS is_insertable_into,
        CASE
            WHEN t.typname IS NOT NULL THEN 'YES'::text
            ELSE 'NO'::text
        END::information_schema.yes_or_no AS is_typed,
    NULL::character varying::information_schema.character_data AS commit_action
   FROM pg_namespace nc
     JOIN pg_class c ON nc.oid = c.relnamespace
     LEFT JOIN (pg_type t
     JOIN pg_namespace nt ON t.typnamespace = nt.oid) ON c.reloftype = t.oid
  WHERE (c.relkind = ANY (ARRAY['r'::"char", 'v'::"char", 'f'::"char", 'p'::"char"])) AND NOT pg_is_other_temp_schema(nc.oid) AND (pg_has_role(c.relowner, 'USAGE'::text) OR has_table_privilege(c.oid, 'SELECT, INSERT, UPDATE, DELETE, TRUNCATE, REFERENCES, TRIGGER'::text) OR has_any_column_privilege(c.oid, 'SELECT, INSERT, UPDATE, REFERENCES'::text));
;

CREATE VIEW information_schema.transforms 
as 
 SELECT current_database()::information_schema.sql_identifier AS udt_catalog,
    nt.nspname::information_schema.sql_identifier AS udt_schema,
    t.typname::information_schema.sql_identifier AS udt_name,
    current_database()::information_schema.sql_identifier AS specific_catalog,
    np.nspname::information_schema.sql_identifier AS specific_schema,
    nameconcatoid(p.proname, p.oid)::information_schema.sql_identifier AS specific_name,
    l.lanname::information_schema.sql_identifier AS group_name,
    'FROM SQL'::character varying::information_schema.character_data AS transform_type
   FROM pg_type t
     JOIN pg_transform x ON t.oid = x.trftype
     JOIN pg_language l ON x.trflang = l.oid
     JOIN pg_proc p ON x.trffromsql::oid = p.oid
     JOIN pg_namespace nt ON t.typnamespace = nt.oid
     JOIN pg_namespace np ON p.pronamespace = np.oid
UNION
 SELECT current_database()::information_schema.sql_identifier AS udt_catalog,
    nt.nspname::information_schema.sql_identifier AS udt_schema,
    t.typname::information_schema.sql_identifier AS udt_name,
    current_database()::information_schema.sql_identifier AS specific_catalog,
    np.nspname::information_schema.sql_identifier AS specific_schema,
    nameconcatoid(p.proname, p.oid)::information_schema.sql_identifier AS specific_name,
    l.lanname::information_schema.sql_identifier AS group_name,
    'TO SQL'::character varying::information_schema.character_data AS transform_type
   FROM pg_type t
     JOIN pg_transform x ON t.oid = x.trftype
     JOIN pg_language l ON x.trflang = l.oid
     JOIN pg_proc p ON x.trftosql::oid = p.oid
     JOIN pg_namespace nt ON t.typnamespace = nt.oid
     JOIN pg_namespace np ON p.pronamespace = np.oid
  ORDER BY 1, 2, 3, 7, 8;
;

CREATE VIEW information_schema.triggered_update_columns 
as 
 SELECT current_database()::information_schema.sql_identifier AS trigger_catalog,
    n.nspname::information_schema.sql_identifier AS trigger_schema,
    t.tgname::information_schema.sql_identifier AS trigger_name,
    current_database()::information_schema.sql_identifier AS event_object_catalog,
    n.nspname::information_schema.sql_identifier AS event_object_schema,
    c.relname::information_schema.sql_identifier AS event_object_table,
    a.attname::information_schema.sql_identifier AS event_object_column
   FROM pg_namespace n,
    pg_class c,
    pg_trigger t,
    ( SELECT ta0.tgoid,
            (ta0.tgat).x AS tgattnum,
            (ta0.tgat).n AS tgattpos
           FROM ( SELECT pg_trigger.oid AS tgoid,
                    information_schema._pg_expandarray(pg_trigger.tgattr) AS tgat
                   FROM pg_trigger) ta0) ta,
    pg_attribute a
  WHERE n.oid = c.relnamespace AND c.oid = t.tgrelid AND t.oid = ta.tgoid AND a.attrelid = t.tgrelid AND a.attnum = ta.tgattnum AND NOT t.tgisinternal AND NOT pg_is_other_temp_schema(n.oid) AND (pg_has_role(c.relowner, 'USAGE'::text) OR has_column_privilege(c.oid, a.attnum, 'INSERT, UPDATE, REFERENCES'::text));
;

CREATE VIEW information_schema.triggers 
as 
 SELECT current_database()::information_schema.sql_identifier AS trigger_catalog,
    n.nspname::information_schema.sql_identifier AS trigger_schema,
    t.tgname::information_schema.sql_identifier AS trigger_name,
    em.text::information_schema.character_data AS event_manipulation,
    current_database()::information_schema.sql_identifier AS event_object_catalog,
    n.nspname::information_schema.sql_identifier AS event_object_schema,
    c.relname::information_schema.sql_identifier AS event_object_table,
    rank() OVER (PARTITION BY (n.nspname::information_schema.sql_identifier), (c.relname::information_schema.sql_identifier), em.num, (t.tgtype::integer & 1), (t.tgtype::integer & 66) ORDER BY t.tgname)::information_schema.cardinal_number AS action_order,
        CASE
            WHEN pg_has_role(c.relowner, 'USAGE'::text) THEN (regexp_match(pg_get_triggerdef(t.oid), '.{35,} WHEN \((.+)\) EXECUTE FUNCTION'::text))[1]
            ELSE NULL::text
        END::information_schema.character_data AS action_condition,
    SUBSTRING(pg_get_triggerdef(t.oid) FROM POSITION(('EXECUTE FUNCTION'::text) IN (SUBSTRING(pg_get_triggerdef(t.oid) FROM 48))) + 47)::information_schema.character_data AS action_statement,
        CASE t.tgtype::integer & 1
            WHEN 1 THEN 'ROW'::text
            ELSE 'STATEMENT'::text
        END::information_schema.character_data AS action_orientation,
        CASE t.tgtype::integer & 66
            WHEN 2 THEN 'BEFORE'::text
            WHEN 64 THEN 'INSTEAD OF'::text
            ELSE 'AFTER'::text
        END::information_schema.character_data AS action_timing,
    t.tgoldtable::information_schema.sql_identifier AS action_reference_old_table,
    t.tgnewtable::information_schema.sql_identifier AS action_reference_new_table,
    NULL::name::information_schema.sql_identifier AS action_reference_old_row,
    NULL::name::information_schema.sql_identifier AS action_reference_new_row,
    NULL::timestamp with time zone::information_schema.time_stamp AS created
   FROM pg_namespace n,
    pg_class c,
    pg_trigger t,
    ( VALUES (4,'INSERT'::text), (8,'DELETE'::text), (16,'UPDATE'::text)) em(num, text)
  WHERE n.oid = c.relnamespace AND c.oid = t.tgrelid AND (t.tgtype::integer & em.num) <> 0 AND NOT t.tgisinternal AND NOT pg_is_other_temp_schema(n.oid) AND (pg_has_role(c.relowner, 'USAGE'::text) OR has_table_privilege(c.oid, 'INSERT, UPDATE, DELETE, TRUNCATE, REFERENCES, TRIGGER'::text) OR has_any_column_privilege(c.oid, 'INSERT, UPDATE, REFERENCES'::text));
;

CREATE VIEW information_schema.udt_privileges 
as 
 SELECT u_grantor.rolname::information_schema.sql_identifier AS grantor,
    grantee.rolname::information_schema.sql_identifier AS grantee,
    current_database()::information_schema.sql_identifier AS udt_catalog,
    n.nspname::information_schema.sql_identifier AS udt_schema,
    t.typname::information_schema.sql_identifier AS udt_name,
    'TYPE USAGE'::character varying::information_schema.character_data AS privilege_type,
        CASE
            WHEN pg_has_role(grantee.oid, t.typowner, 'USAGE'::text) OR t.grantable THEN 'YES'::text
            ELSE 'NO'::text
        END::information_schema.yes_or_no AS is_grantable
   FROM ( SELECT pg_type.oid,
            pg_type.typname,
            pg_type.typnamespace,
            pg_type.typtype,
            pg_type.typowner,
            (aclexplode(COALESCE(pg_type.typacl, acldefault('T'::"char", pg_type.typowner)))).grantor AS grantor,
            (aclexplode(COALESCE(pg_type.typacl, acldefault('T'::"char", pg_type.typowner)))).grantee AS grantee,
            (aclexplode(COALESCE(pg_type.typacl, acldefault('T'::"char", pg_type.typowner)))).privilege_type AS privilege_type,
            (aclexplode(COALESCE(pg_type.typacl, acldefault('T'::"char", pg_type.typowner)))).is_grantable AS is_grantable
           FROM pg_type) t(oid, typname, typnamespace, typtype, typowner, grantor, grantee, prtype, grantable),
    pg_namespace n,
    pg_authid u_grantor,
    ( SELECT pg_authid.oid,
            pg_authid.rolname
           FROM pg_authid
        UNION ALL
         SELECT 0::oid AS oid,
            'PUBLIC'::name) grantee(oid, rolname)
  WHERE t.typnamespace = n.oid AND t.typtype = 'c'::"char" AND t.grantee = grantee.oid AND t.grantor = u_grantor.oid AND t.prtype = 'USAGE'::text AND (pg_has_role(u_grantor.oid, 'USAGE'::text) OR pg_has_role(grantee.oid, 'USAGE'::text) OR grantee.rolname = 'PUBLIC'::name);
;

CREATE VIEW information_schema.usage_privileges 
as 
 SELECT u.rolname::information_schema.sql_identifier AS grantor,
    'PUBLIC'::name::information_schema.sql_identifier AS grantee,
    current_database()::information_schema.sql_identifier AS object_catalog,
    n.nspname::information_schema.sql_identifier AS object_schema,
    c.collname::information_schema.sql_identifier AS object_name,
    'COLLATION'::character varying::information_schema.character_data AS object_type,
    'USAGE'::character varying::information_schema.character_data AS privilege_type,
    'NO'::character varying::information_schema.yes_or_no AS is_grantable
   FROM pg_authid u,
    pg_namespace n,
    pg_collation c
  WHERE u.oid = c.collowner AND c.collnamespace = n.oid AND (c.collencoding = ANY (ARRAY['-1'::integer, ( SELECT pg_database.encoding
           FROM pg_database
          WHERE pg_database.datname = current_database())]))
UNION ALL
 SELECT u_grantor.rolname::information_schema.sql_identifier AS grantor,
    grantee.rolname::information_schema.sql_identifier AS grantee,
    current_database()::information_schema.sql_identifier AS object_catalog,
    n.nspname::information_schema.sql_identifier AS object_schema,
    t.typname::information_schema.sql_identifier AS object_name,
    'DOMAIN'::character varying::information_schema.character_data AS object_type,
    'USAGE'::character varying::information_schema.character_data AS privilege_type,
        CASE
            WHEN pg_has_role(grantee.oid, t.typowner, 'USAGE'::text) OR t.grantable THEN 'YES'::text
            ELSE 'NO'::text
        END::information_schema.yes_or_no AS is_grantable
   FROM ( SELECT pg_type.oid,
            pg_type.typname,
            pg_type.typnamespace,
            pg_type.typtype,
            pg_type.typowner,
            (aclexplode(COALESCE(pg_type.typacl, acldefault('T'::"char", pg_type.typowner)))).grantor AS grantor,
            (aclexplode(COALESCE(pg_type.typacl, acldefault('T'::"char", pg_type.typowner)))).grantee AS grantee,
            (aclexplode(COALESCE(pg_type.typacl, acldefault('T'::"char", pg_type.typowner)))).privilege_type AS privilege_type,
            (aclexplode(COALESCE(pg_type.typacl, acldefault('T'::"char", pg_type.typowner)))).is_grantable AS is_grantable
           FROM pg_type) t(oid, typname, typnamespace, typtype, typowner, grantor, grantee, prtype, grantable),
    pg_namespace n,
    pg_authid u_grantor,
    ( SELECT pg_authid.oid,
            pg_authid.rolname
           FROM pg_authid
        UNION ALL
         SELECT 0::oid AS oid,
            'PUBLIC'::name) grantee(oid, rolname)
  WHERE t.typnamespace = n.oid AND t.typtype = 'd'::"char" AND t.grantee = grantee.oid AND t.grantor = u_grantor.oid AND t.prtype = 'USAGE'::text AND (pg_has_role(u_grantor.oid, 'USAGE'::text) OR pg_has_role(grantee.oid, 'USAGE'::text) OR grantee.rolname = 'PUBLIC'::name)
UNION ALL
 SELECT u_grantor.rolname::information_schema.sql_identifier AS grantor,
    grantee.rolname::information_schema.sql_identifier AS grantee,
    current_database()::information_schema.sql_identifier AS object_catalog,
    ''::name::information_schema.sql_identifier AS object_schema,
    fdw.fdwname::information_schema.sql_identifier AS object_name,
    'FOREIGN DATA WRAPPER'::character varying::information_schema.character_data AS object_type,
    'USAGE'::character varying::information_schema.character_data AS privilege_type,
        CASE
            WHEN pg_has_role(grantee.oid, fdw.fdwowner, 'USAGE'::text) OR fdw.grantable THEN 'YES'::text
            ELSE 'NO'::text
        END::information_schema.yes_or_no AS is_grantable
   FROM ( SELECT pg_foreign_data_wrapper.fdwname,
            pg_foreign_data_wrapper.fdwowner,
            (aclexplode(COALESCE(pg_foreign_data_wrapper.fdwacl, acldefault('F'::"char", pg_foreign_data_wrapper.fdwowner)))).grantor AS grantor,
            (aclexplode(COALESCE(pg_foreign_data_wrapper.fdwacl, acldefault('F'::"char", pg_foreign_data_wrapper.fdwowner)))).grantee AS grantee,
            (aclexplode(COALESCE(pg_foreign_data_wrapper.fdwacl, acldefault('F'::"char", pg_foreign_data_wrapper.fdwowner)))).privilege_type AS privilege_type,
            (aclexplode(COALESCE(pg_foreign_data_wrapper.fdwacl, acldefault('F'::"char", pg_foreign_data_wrapper.fdwowner)))).is_grantable AS is_grantable
           FROM pg_foreign_data_wrapper) fdw(fdwname, fdwowner, grantor, grantee, prtype, grantable),
    pg_authid u_grantor,
    ( SELECT pg_authid.oid,
            pg_authid.rolname
           FROM pg_authid
        UNION ALL
         SELECT 0::oid AS oid,
            'PUBLIC'::name) grantee(oid, rolname)
  WHERE u_grantor.oid = fdw.grantor AND grantee.oid = fdw.grantee AND fdw.prtype = 'USAGE'::text AND (pg_has_role(u_grantor.oid, 'USAGE'::text) OR pg_has_role(grantee.oid, 'USAGE'::text) OR grantee.rolname = 'PUBLIC'::name)
UNION ALL
 SELECT u_grantor.rolname::information_schema.sql_identifier AS grantor,
    grantee.rolname::information_schema.sql_identifier AS grantee,
    current_database()::information_schema.sql_identifier AS object_catalog,
    ''::name::information_schema.sql_identifier AS object_schema,
    srv.srvname::information_schema.sql_identifier AS object_name,
    'FOREIGN SERVER'::character varying::information_schema.character_data AS object_type,
    'USAGE'::character varying::information_schema.character_data AS privilege_type,
        CASE
            WHEN pg_has_role(grantee.oid, srv.srvowner, 'USAGE'::text) OR srv.grantable THEN 'YES'::text
            ELSE 'NO'::text
        END::information_schema.yes_or_no AS is_grantable
   FROM ( SELECT pg_foreign_server.srvname,
            pg_foreign_server.srvowner,
            (aclexplode(COALESCE(pg_foreign_server.srvacl, acldefault('S'::"char", pg_foreign_server.srvowner)))).grantor AS grantor,
            (aclexplode(COALESCE(pg_foreign_server.srvacl, acldefault('S'::"char", pg_foreign_server.srvowner)))).grantee AS grantee,
            (aclexplode(COALESCE(pg_foreign_server.srvacl, acldefault('S'::"char", pg_foreign_server.srvowner)))).privilege_type AS privilege_type,
            (aclexplode(COALESCE(pg_foreign_server.srvacl, acldefault('S'::"char", pg_foreign_server.srvowner)))).is_grantable AS is_grantable
           FROM pg_foreign_server) srv(srvname, srvowner, grantor, grantee, prtype, grantable),
    pg_authid u_grantor,
    ( SELECT pg_authid.oid,
            pg_authid.rolname
           FROM pg_authid
        UNION ALL
         SELECT 0::oid AS oid,
            'PUBLIC'::name) grantee(oid, rolname)
  WHERE u_grantor.oid = srv.grantor AND grantee.oid = srv.grantee AND srv.prtype = 'USAGE'::text AND (pg_has_role(u_grantor.oid, 'USAGE'::text) OR pg_has_role(grantee.oid, 'USAGE'::text) OR grantee.rolname = 'PUBLIC'::name)
UNION ALL
 SELECT u_grantor.rolname::information_schema.sql_identifier AS grantor,
    grantee.rolname::information_schema.sql_identifier AS grantee,
    current_database()::information_schema.sql_identifier AS object_catalog,
    n.nspname::information_schema.sql_identifier AS object_schema,
    c.relname::information_schema.sql_identifier AS object_name,
    'SEQUENCE'::character varying::information_schema.character_data AS object_type,
    'USAGE'::character varying::information_schema.character_data AS privilege_type,
        CASE
            WHEN pg_has_role(grantee.oid, c.relowner, 'USAGE'::text) OR c.grantable THEN 'YES'::text
            ELSE 'NO'::text
        END::information_schema.yes_or_no AS is_grantable
   FROM ( SELECT pg_class.oid,
            pg_class.relname,
            pg_class.relnamespace,
            pg_class.relkind,
            pg_class.relowner,
            (aclexplode(COALESCE(pg_class.relacl, acldefault('r'::"char", pg_class.relowner)))).grantor AS grantor,
            (aclexplode(COALESCE(pg_class.relacl, acldefault('r'::"char", pg_class.relowner)))).grantee AS grantee,
            (aclexplode(COALESCE(pg_class.relacl, acldefault('r'::"char", pg_class.relowner)))).privilege_type AS privilege_type,
            (aclexplode(COALESCE(pg_class.relacl, acldefault('r'::"char", pg_class.relowner)))).is_grantable AS is_grantable
           FROM pg_class) c(oid, relname, relnamespace, relkind, relowner, grantor, grantee, prtype, grantable),
    pg_namespace n,
    pg_authid u_grantor,
    ( SELECT pg_authid.oid,
            pg_authid.rolname
           FROM pg_authid
        UNION ALL
         SELECT 0::oid AS oid,
            'PUBLIC'::name) grantee(oid, rolname)
  WHERE c.relnamespace = n.oid AND c.relkind = 'S'::"char" AND c.grantee = grantee.oid AND c.grantor = u_grantor.oid AND c.prtype = 'USAGE'::text AND (pg_has_role(u_grantor.oid, 'USAGE'::text) OR pg_has_role(grantee.oid, 'USAGE'::text) OR grantee.rolname = 'PUBLIC'::name);
;

CREATE VIEW information_schema.user_defined_types 
as 
 SELECT current_database()::information_schema.sql_identifier AS user_defined_type_catalog,
    n.nspname::information_schema.sql_identifier AS user_defined_type_schema,
    c.relname::information_schema.sql_identifier AS user_defined_type_name,
    'STRUCTURED'::character varying::information_schema.character_data AS user_defined_type_category,
    'YES'::character varying::information_schema.yes_or_no AS is_instantiable,
    NULL::character varying::information_schema.yes_or_no AS is_final,
    NULL::character varying::information_schema.character_data AS ordering_form,
    NULL::character varying::information_schema.character_data AS ordering_category,
    NULL::name::information_schema.sql_identifier AS ordering_routine_catalog,
    NULL::name::information_schema.sql_identifier AS ordering_routine_schema,
    NULL::name::information_schema.sql_identifier AS ordering_routine_name,
    NULL::character varying::information_schema.character_data AS reference_type,
    NULL::character varying::information_schema.character_data AS data_type,
    NULL::integer::information_schema.cardinal_number AS character_maximum_length,
    NULL::integer::information_schema.cardinal_number AS character_octet_length,
    NULL::name::information_schema.sql_identifier AS character_set_catalog,
    NULL::name::information_schema.sql_identifier AS character_set_schema,
    NULL::name::information_schema.sql_identifier AS character_set_name,
    NULL::name::information_schema.sql_identifier AS collation_catalog,
    NULL::name::information_schema.sql_identifier AS collation_schema,
    NULL::name::information_schema.sql_identifier AS collation_name,
    NULL::integer::information_schema.cardinal_number AS numeric_precision,
    NULL::integer::information_schema.cardinal_number AS numeric_precision_radix,
    NULL::integer::information_schema.cardinal_number AS numeric_scale,
    NULL::integer::information_schema.cardinal_number AS datetime_precision,
    NULL::character varying::information_schema.character_data AS interval_type,
    NULL::integer::information_schema.cardinal_number AS interval_precision,
    NULL::name::information_schema.sql_identifier AS source_dtd_identifier,
    NULL::name::information_schema.sql_identifier AS ref_dtd_identifier
   FROM pg_namespace n,
    pg_class c,
    pg_type t
  WHERE n.oid = c.relnamespace AND t.typrelid = c.oid AND c.relkind = 'c'::"char" AND (pg_has_role(t.typowner, 'USAGE'::text) OR has_type_privilege(t.oid, 'USAGE'::text));
;

CREATE VIEW information_schema.view_column_usage 
as 
 SELECT DISTINCT current_database()::information_schema.sql_identifier AS view_catalog,
    nv.nspname::information_schema.sql_identifier AS view_schema,
    v.relname::information_schema.sql_identifier AS view_name,
    current_database()::information_schema.sql_identifier AS table_catalog,
    nt.nspname::information_schema.sql_identifier AS table_schema,
    t.relname::information_schema.sql_identifier AS table_name,
    a.attname::information_schema.sql_identifier AS column_name
   FROM pg_namespace nv,
    pg_class v,
    pg_depend dv,
    pg_depend dt,
    pg_class t,
    pg_namespace nt,
    pg_attribute a
  WHERE nv.oid = v.relnamespace AND v.relkind = 'v'::"char" AND v.oid = dv.refobjid AND dv.refclassid = 'pg_class'::regclass::oid AND dv.classid = 'pg_rewrite'::regclass::oid AND dv.deptype = 'i'::"char" AND dv.objid = dt.objid AND dv.refobjid <> dt.refobjid AND dt.classid = 'pg_rewrite'::regclass::oid AND dt.refclassid = 'pg_class'::regclass::oid AND dt.refobjid = t.oid AND t.relnamespace = nt.oid AND (t.relkind = ANY (ARRAY['r'::"char", 'v'::"char", 'f'::"char", 'p'::"char"])) AND t.oid = a.attrelid AND dt.refobjsubid = a.attnum AND pg_has_role(t.relowner, 'USAGE'::text);
;

CREATE VIEW information_schema.view_routine_usage 
as 
 SELECT DISTINCT current_database()::information_schema.sql_identifier AS table_catalog,
    nv.nspname::information_schema.sql_identifier AS table_schema,
    v.relname::information_schema.sql_identifier AS table_name,
    current_database()::information_schema.sql_identifier AS specific_catalog,
    np.nspname::information_schema.sql_identifier AS specific_schema,
    nameconcatoid(p.proname, p.oid)::information_schema.sql_identifier AS specific_name
   FROM pg_namespace nv,
    pg_class v,
    pg_depend dv,
    pg_depend dp,
    pg_proc p,
    pg_namespace np
  WHERE nv.oid = v.relnamespace AND v.relkind = 'v'::"char" AND v.oid = dv.refobjid AND dv.refclassid = 'pg_class'::regclass::oid AND dv.classid = 'pg_rewrite'::regclass::oid AND dv.deptype = 'i'::"char" AND dv.objid = dp.objid AND dp.classid = 'pg_rewrite'::regclass::oid AND dp.refclassid = 'pg_proc'::regclass::oid AND dp.refobjid = p.oid AND p.pronamespace = np.oid AND pg_has_role(p.proowner, 'USAGE'::text);
;

CREATE VIEW information_schema.view_table_usage 
as 
 SELECT DISTINCT current_database()::information_schema.sql_identifier AS view_catalog,
    nv.nspname::information_schema.sql_identifier AS view_schema,
    v.relname::information_schema.sql_identifier AS view_name,
    current_database()::information_schema.sql_identifier AS table_catalog,
    nt.nspname::information_schema.sql_identifier AS table_schema,
    t.relname::information_schema.sql_identifier AS table_name
   FROM pg_namespace nv,
    pg_class v,
    pg_depend dv,
    pg_depend dt,
    pg_class t,
    pg_namespace nt
  WHERE nv.oid = v.relnamespace AND v.relkind = 'v'::"char" AND v.oid = dv.refobjid AND dv.refclassid = 'pg_class'::regclass::oid AND dv.classid = 'pg_rewrite'::regclass::oid AND dv.deptype = 'i'::"char" AND dv.objid = dt.objid AND dv.refobjid <> dt.refobjid AND dt.classid = 'pg_rewrite'::regclass::oid AND dt.refclassid = 'pg_class'::regclass::oid AND dt.refobjid = t.oid AND t.relnamespace = nt.oid AND (t.relkind = ANY (ARRAY['r'::"char", 'v'::"char", 'f'::"char", 'p'::"char"])) AND pg_has_role(t.relowner, 'USAGE'::text);
;

CREATE VIEW information_schema.views 
as 
 SELECT current_database()::information_schema.sql_identifier AS table_catalog,
    nc.nspname::information_schema.sql_identifier AS table_schema,
    c.relname::information_schema.sql_identifier AS table_name,
        CASE
            WHEN pg_has_role(c.relowner, 'USAGE'::text) THEN pg_get_viewdef(c.oid)
            ELSE NULL::text
        END::information_schema.character_data AS view_definition,
        CASE
            WHEN 'check_option=cascaded'::text = ANY (c.reloptions) THEN 'CASCADED'::text
            WHEN 'check_option=local'::text = ANY (c.reloptions) THEN 'LOCAL'::text
            ELSE 'NONE'::text
        END::information_schema.character_data AS check_option,
        CASE
            WHEN (pg_relation_is_updatable(c.oid::regclass, false) & 20) = 20 THEN 'YES'::text
            ELSE 'NO'::text
        END::information_schema.yes_or_no AS is_updatable,
        CASE
            WHEN (pg_relation_is_updatable(c.oid::regclass, false) & 8) = 8 THEN 'YES'::text
            ELSE 'NO'::text
        END::information_schema.yes_or_no AS is_insertable_into,
        CASE
            WHEN (EXISTS ( SELECT 1
               FROM pg_trigger
              WHERE pg_trigger.tgrelid = c.oid AND (pg_trigger.tgtype::integer & 81) = 81)) THEN 'YES'::text
            ELSE 'NO'::text
        END::information_schema.yes_or_no AS is_trigger_updatable,
        CASE
            WHEN (EXISTS ( SELECT 1
               FROM pg_trigger
              WHERE pg_trigger.tgrelid = c.oid AND (pg_trigger.tgtype::integer & 73) = 73)) THEN 'YES'::text
            ELSE 'NO'::text
        END::information_schema.yes_or_no AS is_trigger_deletable,
        CASE
            WHEN (EXISTS ( SELECT 1
               FROM pg_trigger
              WHERE pg_trigger.tgrelid = c.oid AND (pg_trigger.tgtype::integer & 69) = 69)) THEN 'YES'::text
            ELSE 'NO'::text
        END::information_schema.yes_or_no AS is_trigger_insertable_into
   FROM pg_namespace nc,
    pg_class c
  WHERE c.relnamespace = nc.oid AND c.relkind = 'v'::"char" AND NOT pg_is_other_temp_schema(nc.oid) AND (pg_has_role(c.relowner, 'USAGE'::text) OR has_table_privilege(c.oid, 'SELECT, INSERT, UPDATE, DELETE, TRUNCATE, REFERENCES, TRIGGER'::text) OR has_any_column_privilege(c.oid, 'SELECT, INSERT, UPDATE, REFERENCES'::text));
;

CREATE VIEW pg_catalog.pg_available_extension_versions 
as 
 SELECT e.name,
    e.version,
    x.extname IS NOT NULL AS installed,
    e.superuser,
    e.trusted,
    e.relocatable,
    e.schema,
    e.requires,
    e.comment
   FROM pg_available_extension_versions() e(name, version, superuser, trusted, relocatable, schema, requires, comment)
     LEFT JOIN pg_extension x ON e.name = x.extname AND e.version = x.extversion;
;

CREATE VIEW pg_catalog.pg_available_extensions 
as 
 SELECT e.name,
    e.default_version,
    x.extversion AS installed_version,
    e.comment
   FROM pg_available_extensions() e(name, default_version, comment)
     LEFT JOIN pg_extension x ON e.name = x.extname;
;

CREATE VIEW pg_catalog.pg_backend_memory_contexts 
as 
 SELECT name,
    ident,
    parent,
    level,
    total_bytes,
    total_nblocks,
    free_bytes,
    free_chunks,
    used_bytes
   FROM pg_get_backend_memory_contexts() pg_get_backend_memory_contexts(name, ident, parent, level, total_bytes, total_nblocks, free_bytes, free_chunks, used_bytes);
;

CREATE VIEW pg_catalog.pg_config 
as 
 SELECT name,
    setting
   FROM pg_config() pg_config(name, setting);
;

CREATE VIEW pg_catalog.pg_cursors 
as 
 SELECT name,
    statement,
    is_holdable,
    is_binary,
    is_scrollable,
    creation_time
   FROM pg_cursor() c(name, statement, is_holdable, is_binary, is_scrollable, creation_time);
;

CREATE VIEW pg_catalog.pg_file_settings 
as 
 SELECT sourcefile,
    sourceline,
    seqno,
    name,
    setting,
    applied,
    error
   FROM pg_show_all_file_settings() a(sourcefile, sourceline, seqno, name, setting, applied, error);
;

CREATE VIEW pg_catalog.pg_group 
as 
 SELECT rolname AS groname,
    oid AS grosysid,
    ARRAY( SELECT pg_auth_members.member
           FROM pg_auth_members
          WHERE pg_auth_members.roleid = pg_authid.oid) AS grolist
   FROM pg_authid
  WHERE NOT rolcanlogin;
;

CREATE VIEW pg_catalog.pg_hba_file_rules 
as 
 SELECT rule_number,
    file_name,
    line_number,
    type,
    database,
    user_name,
    address,
    netmask,
    auth_method,
    options,
    error
   FROM pg_hba_file_rules() a(rule_number, file_name, line_number, type, database, user_name, address, netmask, auth_method, options, error);
;

CREATE VIEW pg_catalog.pg_ident_file_mappings 
as 
 SELECT map_number,
    file_name,
    line_number,
    map_name,
    sys_name,
    pg_username,
    error
   FROM pg_ident_file_mappings() a(map_number, file_name, line_number, map_name, sys_name, pg_username, error);
;

CREATE VIEW pg_catalog.pg_indexes 
as 
 SELECT n.nspname AS schemaname,
    c.relname AS tablename,
    i.relname AS indexname,
    t.spcname AS tablespace,
    pg_get_indexdef(i.oid) AS indexdef
   FROM pg_index x
     JOIN pg_class c ON c.oid = x.indrelid
     JOIN pg_class i ON i.oid = x.indexrelid
     LEFT JOIN pg_namespace n ON n.oid = c.relnamespace
     LEFT JOIN pg_tablespace t ON t.oid = i.reltablespace
  WHERE (c.relkind = ANY (ARRAY['r'::"char", 'm'::"char", 'p'::"char"])) AND (i.relkind = ANY (ARRAY['i'::"char", 'I'::"char"]));
;

CREATE VIEW pg_catalog.pg_locks 
as 
 SELECT locktype,
    database,
    relation,
    page,
    tuple,
    virtualxid,
    transactionid,
    classid,
    objid,
    objsubid,
    virtualtransaction,
    pid,
    mode,
    granted,
    fastpath,
    waitstart
   FROM pg_lock_status() l(locktype, database, relation, page, tuple, virtualxid, transactionid, classid, objid, objsubid, virtualtransaction, pid, mode, granted, fastpath, waitstart);
;

CREATE VIEW pg_catalog.pg_matviews 
as 
 SELECT n.nspname AS schemaname,
    c.relname AS matviewname,
    pg_get_userbyid(c.relowner) AS matviewowner,
    t.spcname AS tablespace,
    c.relhasindex AS hasindexes,
    c.relispopulated AS ispopulated,
    pg_get_viewdef(c.oid) AS definition
   FROM pg_class c
     LEFT JOIN pg_namespace n ON n.oid = c.relnamespace
     LEFT JOIN pg_tablespace t ON t.oid = c.reltablespace
  WHERE c.relkind = 'm'::"char";
;

CREATE VIEW pg_catalog.pg_policies 
as 
 SELECT n.nspname AS schemaname,
    c.relname AS tablename,
    pol.polname AS policyname,
        CASE
            WHEN pol.polpermissive THEN 'PERMISSIVE'::text
            ELSE 'RESTRICTIVE'::text
        END AS permissive,
        CASE
            WHEN pol.polroles = '{0}'::oid[] THEN string_to_array('public'::text, ''::text)::name[]
            ELSE ARRAY( SELECT pg_authid.rolname
               FROM pg_authid
              WHERE pg_authid.oid = ANY (pol.polroles)
              ORDER BY pg_authid.rolname)
        END AS roles,
        CASE pol.polcmd
            WHEN 'r'::"char" THEN 'SELECT'::text
            WHEN 'a'::"char" THEN 'INSERT'::text
            WHEN 'w'::"char" THEN 'UPDATE'::text
            WHEN 'd'::"char" THEN 'DELETE'::text
            WHEN '*'::"char" THEN 'ALL'::text
            ELSE NULL::text
        END AS cmd,
    pg_get_expr(pol.polqual, pol.polrelid) AS qual,
    pg_get_expr(pol.polwithcheck, pol.polrelid) AS with_check
   FROM pg_policy pol
     JOIN pg_class c ON c.oid = pol.polrelid
     LEFT JOIN pg_namespace n ON n.oid = c.relnamespace;
;

CREATE VIEW pg_catalog.pg_prepared_statements 
as 
 SELECT name,
    statement,
    prepare_time,
    parameter_types,
    result_types,
    from_sql,
    generic_plans,
    custom_plans
   FROM pg_prepared_statement() p(name, statement, prepare_time, parameter_types, result_types, from_sql, generic_plans, custom_plans);
;

CREATE VIEW pg_catalog.pg_prepared_xacts 
as 
 SELECT p.transaction,
    p.gid,
    p.prepared,
    u.rolname AS owner,
    d.datname AS database
   FROM pg_prepared_xact() p(transaction, gid, prepared, ownerid, dbid)
     LEFT JOIN pg_authid u ON p.ownerid = u.oid
     LEFT JOIN pg_database d ON p.dbid = d.oid;
;

CREATE VIEW pg_catalog.pg_publication_tables 
as 
 SELECT p.pubname,
    n.nspname AS schemaname,
    c.relname AS tablename,
    ( SELECT array_agg(a.attname ORDER BY a.attnum) AS array_agg
           FROM pg_attribute a
          WHERE a.attrelid = gpt.relid AND (a.attnum = ANY (gpt.attrs::smallint[]))) AS attnames,
    pg_get_expr(gpt.qual, gpt.relid) AS rowfilter
   FROM pg_publication p,
    LATERAL pg_get_publication_tables(VARIADIC ARRAY[p.pubname::text]) gpt(pubid, relid, attrs, qual),
    pg_class c
     JOIN pg_namespace n ON n.oid = c.relnamespace
  WHERE c.oid = gpt.relid;
;

CREATE VIEW pg_catalog.pg_replication_origin_status 
as 
 SELECT local_id,
    external_id,
    remote_lsn,
    local_lsn
   FROM pg_show_replication_origin_status() pg_show_replication_origin_status(local_id, external_id, remote_lsn, local_lsn);
;

CREATE VIEW pg_catalog.pg_replication_slots 
as 
 SELECT l.slot_name,
    l.plugin,
    l.slot_type,
    l.datoid,
    d.datname AS database,
    l.temporary,
    l.active,
    l.active_pid,
    l.xmin,
    l.catalog_xmin,
    l.restart_lsn,
    l.confirmed_flush_lsn,
    l.wal_status,
    l.safe_wal_size,
    l.two_phase,
    l.inactive_since,
    l.conflicting,
    l.invalidation_reason,
    l.failover,
    l.synced
   FROM pg_get_replication_slots() l(slot_name, plugin, slot_type, datoid, temporary, active, active_pid, xmin, catalog_xmin, restart_lsn, confirmed_flush_lsn, wal_status, safe_wal_size, two_phase, inactive_since, conflicting, invalidation_reason, failover, synced)
     LEFT JOIN pg_database d ON l.datoid = d.oid;
;

CREATE VIEW pg_catalog.pg_roles 
as 
 SELECT pg_authid.rolname,
    pg_authid.rolsuper,
    pg_authid.rolinherit,
    pg_authid.rolcreaterole,
    pg_authid.rolcreatedb,
    pg_authid.rolcanlogin,
    pg_authid.rolreplication,
    pg_authid.rolconnlimit,
    '********'::text AS rolpassword,
    pg_authid.rolvaliduntil,
    pg_authid.rolbypassrls,
    s.setconfig AS rolconfig,
    pg_authid.oid
   FROM pg_authid
     LEFT JOIN pg_db_role_setting s ON pg_authid.oid = s.setrole AND s.setdatabase = 0::oid;
;

CREATE VIEW pg_catalog.pg_rules 
as 
 SELECT n.nspname AS schemaname,
    c.relname AS tablename,
    r.rulename,
    pg_get_ruledef(r.oid) AS definition
   FROM pg_rewrite r
     JOIN pg_class c ON c.oid = r.ev_class
     LEFT JOIN pg_namespace n ON n.oid = c.relnamespace
  WHERE r.rulename <> '_RETURN'::name;
;

CREATE VIEW pg_catalog.pg_seclabels 
as 
 SELECT l.objoid,
    l.classoid,
    l.objsubid,
        CASE
            WHEN rel.relkind = ANY (ARRAY['r'::"char", 'p'::"char"]) THEN 'table'::text
            WHEN rel.relkind = 'v'::"char" THEN 'view'::text
            WHEN rel.relkind = 'm'::"char" THEN 'materialized view'::text
            WHEN rel.relkind = 'S'::"char" THEN 'sequence'::text
            WHEN rel.relkind = 'f'::"char" THEN 'foreign table'::text
            ELSE NULL::text
        END AS objtype,
    rel.relnamespace AS objnamespace,
        CASE
            WHEN pg_table_is_visible(rel.oid) THEN quote_ident(rel.relname::text)
            ELSE (quote_ident(nsp.nspname::text) || '.'::text) || quote_ident(rel.relname::text)
        END AS objname,
    l.provider,
    l.label
   FROM pg_seclabel l
     JOIN pg_class rel ON l.classoid = rel.tableoid AND l.objoid = rel.oid
     JOIN pg_namespace nsp ON rel.relnamespace = nsp.oid
  WHERE l.objsubid = 0
UNION ALL
 SELECT l.objoid,
    l.classoid,
    l.objsubid,
    'column'::text AS objtype,
    rel.relnamespace AS objnamespace,
    (
        CASE
            WHEN pg_table_is_visible(rel.oid) THEN quote_ident(rel.relname::text)
            ELSE (quote_ident(nsp.nspname::text) || '.'::text) || quote_ident(rel.relname::text)
        END || '.'::text) || att.attname::text AS objname,
    l.provider,
    l.label
   FROM pg_seclabel l
     JOIN pg_class rel ON l.classoid = rel.tableoid AND l.objoid = rel.oid
     JOIN pg_attribute att ON rel.oid = att.attrelid AND l.objsubid = att.attnum
     JOIN pg_namespace nsp ON rel.relnamespace = nsp.oid
  WHERE l.objsubid <> 0
UNION ALL
 SELECT l.objoid,
    l.classoid,
    l.objsubid,
        CASE pro.prokind
            WHEN 'a'::"char" THEN 'aggregate'::text
            WHEN 'f'::"char" THEN 'function'::text
            WHEN 'p'::"char" THEN 'procedure'::text
            WHEN 'w'::"char" THEN 'window'::text
            ELSE NULL::text
        END AS objtype,
    pro.pronamespace AS objnamespace,
    ((
        CASE
            WHEN pg_function_is_visible(pro.oid) THEN quote_ident(pro.proname::text)
            ELSE (quote_ident(nsp.nspname::text) || '.'::text) || quote_ident(pro.proname::text)
        END || '('::text) || pg_get_function_arguments(pro.oid)) || ')'::text AS objname,
    l.provider,
    l.label
   FROM pg_seclabel l
     JOIN pg_proc pro ON l.classoid = pro.tableoid AND l.objoid = pro.oid
     JOIN pg_namespace nsp ON pro.pronamespace = nsp.oid
  WHERE l.objsubid = 0
UNION ALL
 SELECT l.objoid,
    l.classoid,
    l.objsubid,
        CASE
            WHEN typ.typtype = 'd'::"char" THEN 'domain'::text
            ELSE 'type'::text
        END AS objtype,
    typ.typnamespace AS objnamespace,
        CASE
            WHEN pg_type_is_visible(typ.oid) THEN quote_ident(typ.typname::text)
            ELSE (quote_ident(nsp.nspname::text) || '.'::text) || quote_ident(typ.typname::text)
        END AS objname,
    l.provider,
    l.label
   FROM pg_seclabel l
     JOIN pg_type typ ON l.classoid = typ.tableoid AND l.objoid = typ.oid
     JOIN pg_namespace nsp ON typ.typnamespace = nsp.oid
  WHERE l.objsubid = 0
UNION ALL
 SELECT l.objoid,
    l.classoid,
    l.objsubid,
    'large object'::text AS objtype,
    NULL::oid AS objnamespace,
    l.objoid::text AS objname,
    l.provider,
    l.label
   FROM pg_seclabel l
     JOIN pg_largeobject_metadata lom ON l.objoid = lom.oid
  WHERE l.classoid = 'pg_largeobject'::regclass::oid AND l.objsubid = 0
UNION ALL
 SELECT l.objoid,
    l.classoid,
    l.objsubid,
    'language'::text AS objtype,
    NULL::oid AS objnamespace,
    quote_ident(lan.lanname::text) AS objname,
    l.provider,
    l.label
   FROM pg_seclabel l
     JOIN pg_language lan ON l.classoid = lan.tableoid AND l.objoid = lan.oid
  WHERE l.objsubid = 0
UNION ALL
 SELECT l.objoid,
    l.classoid,
    l.objsubid,
    'schema'::text AS objtype,
    nsp.oid AS objnamespace,
    quote_ident(nsp.nspname::text) AS objname,
    l.provider,
    l.label
   FROM pg_seclabel l
     JOIN pg_namespace nsp ON l.classoid = nsp.tableoid AND l.objoid = nsp.oid
  WHERE l.objsubid = 0
UNION ALL
 SELECT l.objoid,
    l.classoid,
    l.objsubid,
    'event trigger'::text AS objtype,
    NULL::oid AS objnamespace,
    quote_ident(evt.evtname::text) AS objname,
    l.provider,
    l.label
   FROM pg_seclabel l
     JOIN pg_event_trigger evt ON l.classoid = evt.tableoid AND l.objoid = evt.oid
  WHERE l.objsubid = 0
UNION ALL
 SELECT l.objoid,
    l.classoid,
    l.objsubid,
    'publication'::text AS objtype,
    NULL::oid AS objnamespace,
    quote_ident(p.pubname::text) AS objname,
    l.provider,
    l.label
   FROM pg_seclabel l
     JOIN pg_publication p ON l.classoid = p.tableoid AND l.objoid = p.oid
  WHERE l.objsubid = 0
UNION ALL
 SELECT l.objoid,
    l.classoid,
    0 AS objsubid,
    'subscription'::text AS objtype,
    NULL::oid AS objnamespace,
    quote_ident(s.subname::text) AS objname,
    l.provider,
    l.label
   FROM pg_shseclabel l
     JOIN pg_subscription s ON l.classoid = s.tableoid AND l.objoid = s.oid
UNION ALL
 SELECT l.objoid,
    l.classoid,
    0 AS objsubid,
    'database'::text AS objtype,
    NULL::oid AS objnamespace,
    quote_ident(dat.datname::text) AS objname,
    l.provider,
    l.label
   FROM pg_shseclabel l
     JOIN pg_database dat ON l.classoid = dat.tableoid AND l.objoid = dat.oid
UNION ALL
 SELECT l.objoid,
    l.classoid,
    0 AS objsubid,
    'tablespace'::text AS objtype,
    NULL::oid AS objnamespace,
    quote_ident(spc.spcname::text) AS objname,
    l.provider,
    l.label
   FROM pg_shseclabel l
     JOIN pg_tablespace spc ON l.classoid = spc.tableoid AND l.objoid = spc.oid
UNION ALL
 SELECT l.objoid,
    l.classoid,
    0 AS objsubid,
    'role'::text AS objtype,
    NULL::oid AS objnamespace,
    quote_ident(rol.rolname::text) AS objname,
    l.provider,
    l.label
   FROM pg_shseclabel l
     JOIN pg_authid rol ON l.classoid = rol.tableoid AND l.objoid = rol.oid;
;

CREATE VIEW pg_catalog.pg_sequences 
as 
 SELECT n.nspname AS schemaname,
    c.relname AS sequencename,
    pg_get_userbyid(c.relowner) AS sequenceowner,
    s.seqtypid::regtype AS data_type,
    s.seqstart AS start_value,
    s.seqmin AS min_value,
    s.seqmax AS max_value,
    s.seqincrement AS increment_by,
    s.seqcycle AS cycle,
    s.seqcache AS cache_size,
        CASE
            WHEN has_sequence_privilege(c.oid, 'SELECT,USAGE'::text) THEN pg_sequence_last_value(c.oid::regclass)
            ELSE NULL::bigint
        END AS last_value
   FROM pg_sequence s
     JOIN pg_class c ON c.oid = s.seqrelid
     LEFT JOIN pg_namespace n ON n.oid = c.relnamespace
  WHERE NOT pg_is_other_temp_schema(n.oid) AND c.relkind = 'S'::"char";
;

CREATE VIEW pg_catalog.pg_shadow 
as 
 SELECT pg_authid.rolname AS usename,
    pg_authid.oid AS usesysid,
    pg_authid.rolcreatedb AS usecreatedb,
    pg_authid.rolsuper AS usesuper,
    pg_authid.rolreplication AS userepl,
    pg_authid.rolbypassrls AS usebypassrls,
    pg_authid.rolpassword AS passwd,
    pg_authid.rolvaliduntil AS valuntil,
    s.setconfig AS useconfig
   FROM pg_authid
     LEFT JOIN pg_db_role_setting s ON pg_authid.oid = s.setrole AND s.setdatabase = 0::oid
  WHERE pg_authid.rolcanlogin;
;

CREATE VIEW pg_catalog.pg_shmem_allocations 
as 
 SELECT name,
    off,
    size,
    allocated_size
   FROM pg_get_shmem_allocations() pg_get_shmem_allocations(name, off, size, allocated_size);
;

CREATE VIEW pg_catalog.pg_stat_activity 
as 
 SELECT s.datid,
    d.datname,
    s.pid,
    s.leader_pid,
    s.usesysid,
    u.rolname AS usename,
    s.application_name,
    s.client_addr,
    s.client_hostname,
    s.client_port,
    s.backend_start,
    s.xact_start,
    s.query_start,
    s.state_change,
    s.wait_event_type,
    s.wait_event,
    s.state,
    s.backend_xid,
    s.backend_xmin,
    s.query_id,
    s.query,
    s.backend_type
   FROM pg_stat_get_activity(NULL::integer) s(datid, pid, usesysid, application_name, state, query, wait_event_type, wait_event, xact_start, query_start, backend_start, state_change, client_addr, client_hostname, client_port, backend_xid, backend_xmin, backend_type, ssl, sslversion, sslcipher, sslbits, ssl_client_dn, ssl_client_serial, ssl_issuer_dn, gss_auth, gss_princ, gss_enc, gss_delegation, leader_pid, query_id)
     LEFT JOIN pg_database d ON s.datid = d.oid
     LEFT JOIN pg_authid u ON s.usesysid = u.oid;
;

CREATE VIEW pg_catalog.pg_stat_all_indexes 
as 
 SELECT c.oid AS relid,
    i.oid AS indexrelid,
    n.nspname AS schemaname,
    c.relname,
    i.relname AS indexrelname,
    pg_stat_get_numscans(i.oid) AS idx_scan,
    pg_stat_get_lastscan(i.oid) AS last_idx_scan,
    pg_stat_get_tuples_returned(i.oid) AS idx_tup_read,
    pg_stat_get_tuples_fetched(i.oid) AS idx_tup_fetch
   FROM pg_class c
     JOIN pg_index x ON c.oid = x.indrelid
     JOIN pg_class i ON i.oid = x.indexrelid
     LEFT JOIN pg_namespace n ON n.oid = c.relnamespace
  WHERE c.relkind = ANY (ARRAY['r'::"char", 't'::"char", 'm'::"char"]);
;

CREATE VIEW pg_catalog.pg_stat_all_tables 
as 
 SELECT c.oid AS relid,
    n.nspname AS schemaname,
    c.relname,
    pg_stat_get_numscans(c.oid) AS seq_scan,
    pg_stat_get_lastscan(c.oid) AS last_seq_scan,
    pg_stat_get_tuples_returned(c.oid) AS seq_tup_read,
    sum(pg_stat_get_numscans(i.indexrelid))::bigint AS idx_scan,
    max(pg_stat_get_lastscan(i.indexrelid)) AS last_idx_scan,
    sum(pg_stat_get_tuples_fetched(i.indexrelid))::bigint + pg_stat_get_tuples_fetched(c.oid) AS idx_tup_fetch,
    pg_stat_get_tuples_inserted(c.oid) AS n_tup_ins,
    pg_stat_get_tuples_updated(c.oid) AS n_tup_upd,
    pg_stat_get_tuples_deleted(c.oid) AS n_tup_del,
    pg_stat_get_tuples_hot_updated(c.oid) AS n_tup_hot_upd,
    pg_stat_get_tuples_newpage_updated(c.oid) AS n_tup_newpage_upd,
    pg_stat_get_live_tuples(c.oid) AS n_live_tup,
    pg_stat_get_dead_tuples(c.oid) AS n_dead_tup,
    pg_stat_get_mod_since_analyze(c.oid) AS n_mod_since_analyze,
    pg_stat_get_ins_since_vacuum(c.oid) AS n_ins_since_vacuum,
    pg_stat_get_last_vacuum_time(c.oid) AS last_vacuum,
    pg_stat_get_last_autovacuum_time(c.oid) AS last_autovacuum,
    pg_stat_get_last_analyze_time(c.oid) AS last_analyze,
    pg_stat_get_last_autoanalyze_time(c.oid) AS last_autoanalyze,
    pg_stat_get_vacuum_count(c.oid) AS vacuum_count,
    pg_stat_get_autovacuum_count(c.oid) AS autovacuum_count,
    pg_stat_get_analyze_count(c.oid) AS analyze_count,
    pg_stat_get_autoanalyze_count(c.oid) AS autoanalyze_count
   FROM pg_class c
     LEFT JOIN pg_index i ON c.oid = i.indrelid
     LEFT JOIN pg_namespace n ON n.oid = c.relnamespace
  WHERE c.relkind = ANY (ARRAY['r'::"char", 't'::"char", 'm'::"char", 'p'::"char"])
  GROUP BY c.oid, n.nspname, c.relname;
;

CREATE VIEW pg_catalog.pg_stat_archiver 
as 
 SELECT archived_count,
    last_archived_wal,
    last_archived_time,
    failed_count,
    last_failed_wal,
    last_failed_time,
    stats_reset
   FROM pg_stat_get_archiver() s(archived_count, last_archived_wal, last_archived_time, failed_count, last_failed_wal, last_failed_time, stats_reset);
;

CREATE VIEW pg_catalog.pg_stat_bgwriter 
as 
 SELECT pg_stat_get_bgwriter_buf_written_clean() AS buffers_clean,
    pg_stat_get_bgwriter_maxwritten_clean() AS maxwritten_clean,
    pg_stat_get_buf_alloc() AS buffers_alloc,
    pg_stat_get_bgwriter_stat_reset_time() AS stats_reset;
;

CREATE VIEW pg_catalog.pg_stat_checkpointer 
as 
 SELECT pg_stat_get_checkpointer_num_timed() AS num_timed,
    pg_stat_get_checkpointer_num_requested() AS num_requested,
    pg_stat_get_checkpointer_restartpoints_timed() AS restartpoints_timed,
    pg_stat_get_checkpointer_restartpoints_requested() AS restartpoints_req,
    pg_stat_get_checkpointer_restartpoints_performed() AS restartpoints_done,
    pg_stat_get_checkpointer_write_time() AS write_time,
    pg_stat_get_checkpointer_sync_time() AS sync_time,
    pg_stat_get_checkpointer_buffers_written() AS buffers_written,
    pg_stat_get_checkpointer_stat_reset_time() AS stats_reset;
;

CREATE VIEW pg_catalog.pg_stat_database 
as 
 SELECT oid AS datid,
    datname,
        CASE
            WHEN oid = 0::oid THEN 0
            ELSE pg_stat_get_db_numbackends(oid)
        END AS numbackends,
    pg_stat_get_db_xact_commit(oid) AS xact_commit,
    pg_stat_get_db_xact_rollback(oid) AS xact_rollback,
    pg_stat_get_db_blocks_fetched(oid) - pg_stat_get_db_blocks_hit(oid) AS blks_read,
    pg_stat_get_db_blocks_hit(oid) AS blks_hit,
    pg_stat_get_db_tuples_returned(oid) AS tup_returned,
    pg_stat_get_db_tuples_fetched(oid) AS tup_fetched,
    pg_stat_get_db_tuples_inserted(oid) AS tup_inserted,
    pg_stat_get_db_tuples_updated(oid) AS tup_updated,
    pg_stat_get_db_tuples_deleted(oid) AS tup_deleted,
    pg_stat_get_db_conflict_all(oid) AS conflicts,
    pg_stat_get_db_temp_files(oid) AS temp_files,
    pg_stat_get_db_temp_bytes(oid) AS temp_bytes,
    pg_stat_get_db_deadlocks(oid) AS deadlocks,
    pg_stat_get_db_checksum_failures(oid) AS checksum_failures,
    pg_stat_get_db_checksum_last_failure(oid) AS checksum_last_failure,
    pg_stat_get_db_blk_read_time(oid) AS blk_read_time,
    pg_stat_get_db_blk_write_time(oid) AS blk_write_time,
    pg_stat_get_db_session_time(oid) AS session_time,
    pg_stat_get_db_active_time(oid) AS active_time,
    pg_stat_get_db_idle_in_transaction_time(oid) AS idle_in_transaction_time,
    pg_stat_get_db_sessions(oid) AS sessions,
    pg_stat_get_db_sessions_abandoned(oid) AS sessions_abandoned,
    pg_stat_get_db_sessions_fatal(oid) AS sessions_fatal,
    pg_stat_get_db_sessions_killed(oid) AS sessions_killed,
    pg_stat_get_db_stat_reset_time(oid) AS stats_reset
   FROM ( SELECT 0 AS oid,
            NULL::name AS datname
        UNION ALL
         SELECT pg_database.oid,
            pg_database.datname
           FROM pg_database) d;
;

CREATE VIEW pg_catalog.pg_stat_database_conflicts 
as 
 SELECT oid AS datid,
    datname,
    pg_stat_get_db_conflict_tablespace(oid) AS confl_tablespace,
    pg_stat_get_db_conflict_lock(oid) AS confl_lock,
    pg_stat_get_db_conflict_snapshot(oid) AS confl_snapshot,
    pg_stat_get_db_conflict_bufferpin(oid) AS confl_bufferpin,
    pg_stat_get_db_conflict_startup_deadlock(oid) AS confl_deadlock,
    pg_stat_get_db_conflict_logicalslot(oid) AS confl_active_logicalslot
   FROM pg_database d;
;

CREATE VIEW pg_catalog.pg_stat_gssapi 
as 
 SELECT pid,
    gss_auth AS gss_authenticated,
    gss_princ AS principal,
    gss_enc AS encrypted,
    gss_delegation AS credentials_delegated
   FROM pg_stat_get_activity(NULL::integer) s(datid, pid, usesysid, application_name, state, query, wait_event_type, wait_event, xact_start, query_start, backend_start, state_change, client_addr, client_hostname, client_port, backend_xid, backend_xmin, backend_type, ssl, sslversion, sslcipher, sslbits, ssl_client_dn, ssl_client_serial, ssl_issuer_dn, gss_auth, gss_princ, gss_enc, gss_delegation, leader_pid, query_id)
  WHERE client_port IS NOT NULL;
;

CREATE VIEW pg_catalog.pg_stat_io 
as 
 SELECT backend_type,
    object,
    context,
    reads,
    read_time,
    writes,
    write_time,
    writebacks,
    writeback_time,
    extends,
    extend_time,
    op_bytes,
    hits,
    evictions,
    reuses,
    fsyncs,
    fsync_time,
    stats_reset
   FROM pg_stat_get_io() b(backend_type, object, context, reads, read_time, writes, write_time, writebacks, writeback_time, extends, extend_time, op_bytes, hits, evictions, reuses, fsyncs, fsync_time, stats_reset);
;

CREATE VIEW pg_catalog.pg_stat_progress_analyze 
as 
 SELECT s.pid,
    s.datid,
    d.datname,
    s.relid,
        CASE s.param1
            WHEN 0 THEN 'initializing'::text
            WHEN 1 THEN 'acquiring sample rows'::text
            WHEN 2 THEN 'acquiring inherited sample rows'::text
            WHEN 3 THEN 'computing statistics'::text
            WHEN 4 THEN 'computing extended statistics'::text
            WHEN 5 THEN 'finalizing analyze'::text
            ELSE NULL::text
        END AS phase,
    s.param2 AS sample_blks_total,
    s.param3 AS sample_blks_scanned,
    s.param4 AS ext_stats_total,
    s.param5 AS ext_stats_computed,
    s.param6 AS child_tables_total,
    s.param7 AS child_tables_done,
    s.param8::oid AS current_child_table_relid
   FROM pg_stat_get_progress_info('ANALYZE'::text) s(pid, datid, relid, param1, param2, param3, param4, param5, param6, param7, param8, param9, param10, param11, param12, param13, param14, param15, param16, param17, param18, param19, param20)
     LEFT JOIN pg_database d ON s.datid = d.oid;
;

CREATE VIEW pg_catalog.pg_stat_progress_basebackup 
as 
 SELECT pid,
        CASE param1
            WHEN 0 THEN 'initializing'::text
            WHEN 1 THEN 'waiting for checkpoint to finish'::text
            WHEN 2 THEN 'estimating backup size'::text
            WHEN 3 THEN 'streaming database files'::text
            WHEN 4 THEN 'waiting for wal archiving to finish'::text
            WHEN 5 THEN 'transferring wal files'::text
            ELSE NULL::text
        END AS phase,
        CASE param2
            WHEN '-1'::integer THEN NULL::bigint
            ELSE param2
        END AS backup_total,
    param3 AS backup_streamed,
    param4 AS tablespaces_total,
    param5 AS tablespaces_streamed
   FROM pg_stat_get_progress_info('BASEBACKUP'::text) s(pid, datid, relid, param1, param2, param3, param4, param5, param6, param7, param8, param9, param10, param11, param12, param13, param14, param15, param16, param17, param18, param19, param20);
;

CREATE VIEW pg_catalog.pg_stat_progress_cluster 
as 
 SELECT s.pid,
    s.datid,
    d.datname,
    s.relid,
        CASE s.param1
            WHEN 1 THEN 'CLUSTER'::text
            WHEN 2 THEN 'VACUUM FULL'::text
            ELSE NULL::text
        END AS command,
        CASE s.param2
            WHEN 0 THEN 'initializing'::text
            WHEN 1 THEN 'seq scanning heap'::text
            WHEN 2 THEN 'index scanning heap'::text
            WHEN 3 THEN 'sorting tuples'::text
            WHEN 4 THEN 'writing new heap'::text
            WHEN 5 THEN 'swapping relation files'::text
            WHEN 6 THEN 'rebuilding index'::text
            WHEN 7 THEN 'performing final cleanup'::text
            ELSE NULL::text
        END AS phase,
    s.param3::oid AS cluster_index_relid,
    s.param4 AS heap_tuples_scanned,
    s.param5 AS heap_tuples_written,
    s.param6 AS heap_blks_total,
    s.param7 AS heap_blks_scanned,
    s.param8 AS index_rebuild_count
   FROM pg_stat_get_progress_info('CLUSTER'::text) s(pid, datid, relid, param1, param2, param3, param4, param5, param6, param7, param8, param9, param10, param11, param12, param13, param14, param15, param16, param17, param18, param19, param20)
     LEFT JOIN pg_database d ON s.datid = d.oid;
;

CREATE VIEW pg_catalog.pg_stat_progress_copy 
as 
 SELECT s.pid,
    s.datid,
    d.datname,
    s.relid,
        CASE s.param5
            WHEN 1 THEN 'COPY FROM'::text
            WHEN 2 THEN 'COPY TO'::text
            ELSE NULL::text
        END AS command,
        CASE s.param6
            WHEN 1 THEN 'FILE'::text
            WHEN 2 THEN 'PROGRAM'::text
            WHEN 3 THEN 'PIPE'::text
            WHEN 4 THEN 'CALLBACK'::text
            ELSE NULL::text
        END AS type,
    s.param1 AS bytes_processed,
    s.param2 AS bytes_total,
    s.param3 AS tuples_processed,
    s.param4 AS tuples_excluded,
    s.param7 AS tuples_skipped
   FROM pg_stat_get_progress_info('COPY'::text) s(pid, datid, relid, param1, param2, param3, param4, param5, param6, param7, param8, param9, param10, param11, param12, param13, param14, param15, param16, param17, param18, param19, param20)
     LEFT JOIN pg_database d ON s.datid = d.oid;
;

CREATE VIEW pg_catalog.pg_stat_progress_create_index 
as 
 SELECT s.pid,
    s.datid,
    d.datname,
    s.relid,
    s.param7::oid AS index_relid,
        CASE s.param1
            WHEN 1 THEN 'CREATE INDEX'::text
            WHEN 2 THEN 'CREATE INDEX CONCURRENTLY'::text
            WHEN 3 THEN 'REINDEX'::text
            WHEN 4 THEN 'REINDEX CONCURRENTLY'::text
            ELSE NULL::text
        END AS command,
        CASE s.param10
            WHEN 0 THEN 'initializing'::text
            WHEN 1 THEN 'waiting for writers before build'::text
            WHEN 2 THEN 'building index'::text || COALESCE(': '::text || pg_indexam_progress_phasename(s.param9::oid, s.param11), ''::text)
            WHEN 3 THEN 'waiting for writers before validation'::text
            WHEN 4 THEN 'index validation: scanning index'::text
            WHEN 5 THEN 'index validation: sorting tuples'::text
            WHEN 6 THEN 'index validation: scanning table'::text
            WHEN 7 THEN 'waiting for old snapshots'::text
            WHEN 8 THEN 'waiting for readers before marking dead'::text
            WHEN 9 THEN 'waiting for readers before dropping'::text
            ELSE NULL::text
        END AS phase,
    s.param4 AS lockers_total,
    s.param5 AS lockers_done,
    s.param6 AS current_locker_pid,
    s.param16 AS blocks_total,
    s.param17 AS blocks_done,
    s.param12 AS tuples_total,
    s.param13 AS tuples_done,
    s.param14 AS partitions_total,
    s.param15 AS partitions_done
   FROM pg_stat_get_progress_info('CREATE INDEX'::text) s(pid, datid, relid, param1, param2, param3, param4, param5, param6, param7, param8, param9, param10, param11, param12, param13, param14, param15, param16, param17, param18, param19, param20)
     LEFT JOIN pg_database d ON s.datid = d.oid;
;

CREATE VIEW pg_catalog.pg_stat_progress_vacuum 
as 
 SELECT s.pid,
    s.datid,
    d.datname,
    s.relid,
        CASE s.param1
            WHEN 0 THEN 'initializing'::text
            WHEN 1 THEN 'scanning heap'::text
            WHEN 2 THEN 'vacuuming indexes'::text
            WHEN 3 THEN 'vacuuming heap'::text
            WHEN 4 THEN 'cleaning up indexes'::text
            WHEN 5 THEN 'truncating heap'::text
            WHEN 6 THEN 'performing final cleanup'::text
            ELSE NULL::text
        END AS phase,
    s.param2 AS heap_blks_total,
    s.param3 AS heap_blks_scanned,
    s.param4 AS heap_blks_vacuumed,
    s.param5 AS index_vacuum_count,
    s.param6 AS max_dead_tuple_bytes,
    s.param7 AS dead_tuple_bytes,
    s.param8 AS num_dead_item_ids,
    s.param9 AS indexes_total,
    s.param10 AS indexes_processed
   FROM pg_stat_get_progress_info('VACUUM'::text) s(pid, datid, relid, param1, param2, param3, param4, param5, param6, param7, param8, param9, param10, param11, param12, param13, param14, param15, param16, param17, param18, param19, param20)
     LEFT JOIN pg_database d ON s.datid = d.oid;
;

CREATE VIEW pg_catalog.pg_stat_recovery_prefetch 
as 
 SELECT stats_reset,
    prefetch,
    hit,
    skip_init,
    skip_new,
    skip_fpw,
    skip_rep,
    wal_distance,
    block_distance,
    io_depth
   FROM pg_stat_get_recovery_prefetch() s(stats_reset, prefetch, hit, skip_init, skip_new, skip_fpw, skip_rep, wal_distance, block_distance, io_depth);
;

CREATE VIEW pg_catalog.pg_stat_replication 
as 
 SELECT s.pid,
    s.usesysid,
    u.rolname AS usename,
    s.application_name,
    s.client_addr,
    s.client_hostname,
    s.client_port,
    s.backend_start,
    s.backend_xmin,
    w.state,
    w.sent_lsn,
    w.write_lsn,
    w.flush_lsn,
    w.replay_lsn,
    w.write_lag,
    w.flush_lag,
    w.replay_lag,
    w.sync_priority,
    w.sync_state,
    w.reply_time
   FROM pg_stat_get_activity(NULL::integer) s(datid, pid, usesysid, application_name, state, query, wait_event_type, wait_event, xact_start, query_start, backend_start, state_change, client_addr, client_hostname, client_port, backend_xid, backend_xmin, backend_type, ssl, sslversion, sslcipher, sslbits, ssl_client_dn, ssl_client_serial, ssl_issuer_dn, gss_auth, gss_princ, gss_enc, gss_delegation, leader_pid, query_id)
     JOIN pg_stat_get_wal_senders() w(pid, state, sent_lsn, write_lsn, flush_lsn, replay_lsn, write_lag, flush_lag, replay_lag, sync_priority, sync_state, reply_time) ON s.pid = w.pid
     LEFT JOIN pg_authid u ON s.usesysid = u.oid;
;

CREATE VIEW pg_catalog.pg_stat_slru 
as 
 SELECT name,
    blks_zeroed,
    blks_hit,
    blks_read,
    blks_written,
    blks_exists,
    flushes,
    truncates,
    stats_reset
   FROM pg_stat_get_slru() s(name, blks_zeroed, blks_hit, blks_read, blks_written, blks_exists, flushes, truncates, stats_reset);
;

CREATE VIEW pg_catalog.pg_stat_ssl 
as 
 SELECT pid,
    ssl,
    sslversion AS version,
    sslcipher AS cipher,
    sslbits AS bits,
    ssl_client_dn AS client_dn,
    ssl_client_serial AS client_serial,
    ssl_issuer_dn AS issuer_dn
   FROM pg_stat_get_activity(NULL::integer) s(datid, pid, usesysid, application_name, state, query, wait_event_type, wait_event, xact_start, query_start, backend_start, state_change, client_addr, client_hostname, client_port, backend_xid, backend_xmin, backend_type, ssl, sslversion, sslcipher, sslbits, ssl_client_dn, ssl_client_serial, ssl_issuer_dn, gss_auth, gss_princ, gss_enc, gss_delegation, leader_pid, query_id)
  WHERE client_port IS NOT NULL;
;

CREATE VIEW pg_catalog.pg_stat_subscription 
as 
 SELECT su.oid AS subid,
    su.subname,
    st.worker_type,
    st.pid,
    st.leader_pid,
    st.relid,
    st.received_lsn,
    st.last_msg_send_time,
    st.last_msg_receipt_time,
    st.latest_end_lsn,
    st.latest_end_time
   FROM pg_subscription su
     LEFT JOIN pg_stat_get_subscription(NULL::oid) st(subid, relid, pid, leader_pid, received_lsn, last_msg_send_time, last_msg_receipt_time, latest_end_lsn, latest_end_time, worker_type) ON st.subid = su.oid;
;

CREATE VIEW pg_catalog.pg_stat_subscription_stats 
as 
 SELECT ss.subid,
    s.subname,
    ss.apply_error_count,
    ss.sync_error_count,
    ss.stats_reset
   FROM pg_subscription s,
    LATERAL pg_stat_get_subscription_stats(s.oid) ss(subid, apply_error_count, sync_error_count, stats_reset);
;

CREATE VIEW pg_catalog.pg_stat_user_functions 
as 
 SELECT p.oid AS funcid,
    n.nspname AS schemaname,
    p.proname AS funcname,
    pg_stat_get_function_calls(p.oid) AS calls,
    pg_stat_get_function_total_time(p.oid) AS total_time,
    pg_stat_get_function_self_time(p.oid) AS self_time
   FROM pg_proc p
     LEFT JOIN pg_namespace n ON n.oid = p.pronamespace
  WHERE p.prolang <> 12::oid AND pg_stat_get_function_calls(p.oid) IS NOT NULL;
;

CREATE VIEW pg_catalog.pg_stat_wal 
as 
 SELECT wal_records,
    wal_fpi,
    wal_bytes,
    wal_buffers_full,
    wal_write,
    wal_sync,
    wal_write_time,
    wal_sync_time,
    stats_reset
   FROM pg_stat_get_wal() w(wal_records, wal_fpi, wal_bytes, wal_buffers_full, wal_write, wal_sync, wal_write_time, wal_sync_time, stats_reset);
;

CREATE VIEW pg_catalog.pg_stat_wal_receiver 
as 
 SELECT pid,
    status,
    receive_start_lsn,
    receive_start_tli,
    written_lsn,
    flushed_lsn,
    received_tli,
    last_msg_send_time,
    last_msg_receipt_time,
    latest_end_lsn,
    latest_end_time,
    slot_name,
    sender_host,
    sender_port,
    conninfo
   FROM pg_stat_get_wal_receiver() s(pid, status, receive_start_lsn, receive_start_tli, written_lsn, flushed_lsn, received_tli, last_msg_send_time, last_msg_receipt_time, latest_end_lsn, latest_end_time, slot_name, sender_host, sender_port, conninfo)
  WHERE pid IS NOT NULL;
;

CREATE VIEW pg_catalog.pg_stat_xact_all_tables 
as 
 SELECT c.oid AS relid,
    n.nspname AS schemaname,
    c.relname,
    pg_stat_get_xact_numscans(c.oid) AS seq_scan,
    pg_stat_get_xact_tuples_returned(c.oid) AS seq_tup_read,
    sum(pg_stat_get_xact_numscans(i.indexrelid))::bigint AS idx_scan,
    sum(pg_stat_get_xact_tuples_fetched(i.indexrelid))::bigint + pg_stat_get_xact_tuples_fetched(c.oid) AS idx_tup_fetch,
    pg_stat_get_xact_tuples_inserted(c.oid) AS n_tup_ins,
    pg_stat_get_xact_tuples_updated(c.oid) AS n_tup_upd,
    pg_stat_get_xact_tuples_deleted(c.oid) AS n_tup_del,
    pg_stat_get_xact_tuples_hot_updated(c.oid) AS n_tup_hot_upd,
    pg_stat_get_xact_tuples_newpage_updated(c.oid) AS n_tup_newpage_upd
   FROM pg_class c
     LEFT JOIN pg_index i ON c.oid = i.indrelid
     LEFT JOIN pg_namespace n ON n.oid = c.relnamespace
  WHERE c.relkind = ANY (ARRAY['r'::"char", 't'::"char", 'm'::"char", 'p'::"char"])
  GROUP BY c.oid, n.nspname, c.relname;
;

CREATE VIEW pg_catalog.pg_stat_xact_user_functions 
as 
 SELECT p.oid AS funcid,
    n.nspname AS schemaname,
    p.proname AS funcname,
    pg_stat_get_xact_function_calls(p.oid) AS calls,
    pg_stat_get_xact_function_total_time(p.oid) AS total_time,
    pg_stat_get_xact_function_self_time(p.oid) AS self_time
   FROM pg_proc p
     LEFT JOIN pg_namespace n ON n.oid = p.pronamespace
  WHERE p.prolang <> 12::oid AND pg_stat_get_xact_function_calls(p.oid) IS NOT NULL;
;

CREATE VIEW pg_catalog.pg_statio_all_indexes 
as 
 SELECT c.oid AS relid,
    i.oid AS indexrelid,
    n.nspname AS schemaname,
    c.relname,
    i.relname AS indexrelname,
    pg_stat_get_blocks_fetched(i.oid) - pg_stat_get_blocks_hit(i.oid) AS idx_blks_read,
    pg_stat_get_blocks_hit(i.oid) AS idx_blks_hit
   FROM pg_class c
     JOIN pg_index x ON c.oid = x.indrelid
     JOIN pg_class i ON i.oid = x.indexrelid
     LEFT JOIN pg_namespace n ON n.oid = c.relnamespace
  WHERE c.relkind = ANY (ARRAY['r'::"char", 't'::"char", 'm'::"char"]);
;

CREATE VIEW pg_catalog.pg_statio_all_sequences 
as 
 SELECT c.oid AS relid,
    n.nspname AS schemaname,
    c.relname,
    pg_stat_get_blocks_fetched(c.oid) - pg_stat_get_blocks_hit(c.oid) AS blks_read,
    pg_stat_get_blocks_hit(c.oid) AS blks_hit
   FROM pg_class c
     LEFT JOIN pg_namespace n ON n.oid = c.relnamespace
  WHERE c.relkind = 'S'::"char";
;

CREATE VIEW pg_catalog.pg_statio_all_tables 
as 
 SELECT c.oid AS relid,
    n.nspname AS schemaname,
    c.relname,
    pg_stat_get_blocks_fetched(c.oid) - pg_stat_get_blocks_hit(c.oid) AS heap_blks_read,
    pg_stat_get_blocks_hit(c.oid) AS heap_blks_hit,
    i.idx_blks_read,
    i.idx_blks_hit,
    pg_stat_get_blocks_fetched(t.oid) - pg_stat_get_blocks_hit(t.oid) AS toast_blks_read,
    pg_stat_get_blocks_hit(t.oid) AS toast_blks_hit,
    x.idx_blks_read AS tidx_blks_read,
    x.idx_blks_hit AS tidx_blks_hit
   FROM pg_class c
     LEFT JOIN pg_class t ON c.reltoastrelid = t.oid
     LEFT JOIN pg_namespace n ON n.oid = c.relnamespace
     LEFT JOIN LATERAL ( SELECT sum(pg_stat_get_blocks_fetched(pg_index.indexrelid) - pg_stat_get_blocks_hit(pg_index.indexrelid))::bigint AS idx_blks_read,
            sum(pg_stat_get_blocks_hit(pg_index.indexrelid))::bigint AS idx_blks_hit
           FROM pg_index
          WHERE pg_index.indrelid = c.oid) i ON true
     LEFT JOIN LATERAL ( SELECT sum(pg_stat_get_blocks_fetched(pg_index.indexrelid) - pg_stat_get_blocks_hit(pg_index.indexrelid))::bigint AS idx_blks_read,
            sum(pg_stat_get_blocks_hit(pg_index.indexrelid))::bigint AS idx_blks_hit
           FROM pg_index
          WHERE pg_index.indrelid = t.oid) x ON true
  WHERE c.relkind = ANY (ARRAY['r'::"char", 't'::"char", 'm'::"char"]);
;

CREATE VIEW pg_catalog.pg_stats 
as 
 SELECT n.nspname AS schemaname,
    c.relname AS tablename,
    a.attname,
    s.stainherit AS inherited,
    s.stanullfrac AS null_frac,
    s.stawidth AS avg_width,
    s.stadistinct AS n_distinct,
        CASE
            WHEN s.stakind1 = 1 THEN s.stavalues1
            WHEN s.stakind2 = 1 THEN s.stavalues2
            WHEN s.stakind3 = 1 THEN s.stavalues3
            WHEN s.stakind4 = 1 THEN s.stavalues4
            WHEN s.stakind5 = 1 THEN s.stavalues5
            ELSE NULL::anyarray
        END AS most_common_vals,
        CASE
            WHEN s.stakind1 = 1 THEN s.stanumbers1
            WHEN s.stakind2 = 1 THEN s.stanumbers2
            WHEN s.stakind3 = 1 THEN s.stanumbers3
            WHEN s.stakind4 = 1 THEN s.stanumbers4
            WHEN s.stakind5 = 1 THEN s.stanumbers5
            ELSE NULL::real[]
        END AS most_common_freqs,
        CASE
            WHEN s.stakind1 = 2 THEN s.stavalues1
            WHEN s.stakind2 = 2 THEN s.stavalues2
            WHEN s.stakind3 = 2 THEN s.stavalues3
            WHEN s.stakind4 = 2 THEN s.stavalues4
            WHEN s.stakind5 = 2 THEN s.stavalues5
            ELSE NULL::anyarray
        END AS histogram_bounds,
        CASE
            WHEN s.stakind1 = 3 THEN s.stanumbers1[1]
            WHEN s.stakind2 = 3 THEN s.stanumbers2[1]
            WHEN s.stakind3 = 3 THEN s.stanumbers3[1]
            WHEN s.stakind4 = 3 THEN s.stanumbers4[1]
            WHEN s.stakind5 = 3 THEN s.stanumbers5[1]
            ELSE NULL::real
        END AS correlation,
        CASE
            WHEN s.stakind1 = 4 THEN s.stavalues1
            WHEN s.stakind2 = 4 THEN s.stavalues2
            WHEN s.stakind3 = 4 THEN s.stavalues3
            WHEN s.stakind4 = 4 THEN s.stavalues4
            WHEN s.stakind5 = 4 THEN s.stavalues5
            ELSE NULL::anyarray
        END AS most_common_elems,
        CASE
            WHEN s.stakind1 = 4 THEN s.stanumbers1
            WHEN s.stakind2 = 4 THEN s.stanumbers2
            WHEN s.stakind3 = 4 THEN s.stanumbers3
            WHEN s.stakind4 = 4 THEN s.stanumbers4
            WHEN s.stakind5 = 4 THEN s.stanumbers5
            ELSE NULL::real[]
        END AS most_common_elem_freqs,
        CASE
            WHEN s.stakind1 = 5 THEN s.stanumbers1
            WHEN s.stakind2 = 5 THEN s.stanumbers2
            WHEN s.stakind3 = 5 THEN s.stanumbers3
            WHEN s.stakind4 = 5 THEN s.stanumbers4
            WHEN s.stakind5 = 5 THEN s.stanumbers5
            ELSE NULL::real[]
        END AS elem_count_histogram,
        CASE
            WHEN s.stakind1 = 6 THEN s.stavalues1
            WHEN s.stakind2 = 6 THEN s.stavalues2
            WHEN s.stakind3 = 6 THEN s.stavalues3
            WHEN s.stakind4 = 6 THEN s.stavalues4
            WHEN s.stakind5 = 6 THEN s.stavalues5
            ELSE NULL::anyarray
        END AS range_length_histogram,
        CASE
            WHEN s.stakind1 = 6 THEN s.stanumbers1[1]
            WHEN s.stakind2 = 6 THEN s.stanumbers2[1]
            WHEN s.stakind3 = 6 THEN s.stanumbers3[1]
            WHEN s.stakind4 = 6 THEN s.stanumbers4[1]
            WHEN s.stakind5 = 6 THEN s.stanumbers5[1]
            ELSE NULL::real
        END AS range_empty_frac,
        CASE
            WHEN s.stakind1 = 7 THEN s.stavalues1
            WHEN s.stakind2 = 7 THEN s.stavalues2
            WHEN s.stakind3 = 7 THEN s.stavalues3
            WHEN s.stakind4 = 7 THEN s.stavalues4
            WHEN s.stakind5 = 7 THEN s.stavalues5
            ELSE NULL::anyarray
        END AS range_bounds_histogram
   FROM pg_statistic s
     JOIN pg_class c ON c.oid = s.starelid
     JOIN pg_attribute a ON c.oid = a.attrelid AND a.attnum = s.staattnum
     LEFT JOIN pg_namespace n ON n.oid = c.relnamespace
  WHERE NOT a.attisdropped AND has_column_privilege(c.oid, a.attnum, 'select'::text) AND (c.relrowsecurity = false OR NOT row_security_active(c.oid));
;

CREATE VIEW pg_catalog.pg_stats_ext 
as 
 SELECT cn.nspname AS schemaname,
    c.relname AS tablename,
    sn.nspname AS statistics_schemaname,
    s.stxname AS statistics_name,
    pg_get_userbyid(s.stxowner) AS statistics_owner,
    ( SELECT array_agg(a.attname ORDER BY a.attnum) AS array_agg
           FROM unnest(s.stxkeys) k(k)
             JOIN pg_attribute a ON a.attrelid = s.stxrelid AND a.attnum = k.k) AS attnames,
    pg_get_statisticsobjdef_expressions(s.oid) AS exprs,
    s.stxkind AS kinds,
    sd.stxdinherit AS inherited,
    sd.stxdndistinct AS n_distinct,
    sd.stxddependencies AS dependencies,
    m.most_common_vals,
    m.most_common_val_nulls,
    m.most_common_freqs,
    m.most_common_base_freqs
   FROM pg_statistic_ext s
     JOIN pg_class c ON c.oid = s.stxrelid
     JOIN pg_statistic_ext_data sd ON s.oid = sd.stxoid
     LEFT JOIN pg_namespace cn ON cn.oid = c.relnamespace
     LEFT JOIN pg_namespace sn ON sn.oid = s.stxnamespace
     LEFT JOIN LATERAL ( SELECT array_agg(pg_mcv_list_items."values") AS most_common_vals,
            array_agg(pg_mcv_list_items.nulls) AS most_common_val_nulls,
            array_agg(pg_mcv_list_items.frequency) AS most_common_freqs,
            array_agg(pg_mcv_list_items.base_frequency) AS most_common_base_freqs
           FROM pg_mcv_list_items(sd.stxdmcv) pg_mcv_list_items(index, "values", nulls, frequency, base_frequency)) m ON sd.stxdmcv IS NOT NULL
  WHERE pg_has_role(c.relowner, 'USAGE'::text) AND (c.relrowsecurity = false OR NOT row_security_active(c.oid));
;

CREATE VIEW pg_catalog.pg_stats_ext_exprs 
as 
 SELECT cn.nspname AS schemaname,
    c.relname AS tablename,
    sn.nspname AS statistics_schemaname,
    s.stxname AS statistics_name,
    pg_get_userbyid(s.stxowner) AS statistics_owner,
    stat.expr,
    sd.stxdinherit AS inherited,
    (stat.a).stanullfrac AS null_frac,
    (stat.a).stawidth AS avg_width,
    (stat.a).stadistinct AS n_distinct,
        CASE
            WHEN (stat.a).stakind1 = 1 THEN (stat.a).stavalues1
            WHEN (stat.a).stakind2 = 1 THEN (stat.a).stavalues2
            WHEN (stat.a).stakind3 = 1 THEN (stat.a).stavalues3
            WHEN (stat.a).stakind4 = 1 THEN (stat.a).stavalues4
            WHEN (stat.a).stakind5 = 1 THEN (stat.a).stavalues5
            ELSE NULL::anyarray
        END AS most_common_vals,
        CASE
            WHEN (stat.a).stakind1 = 1 THEN (stat.a).stanumbers1
            WHEN (stat.a).stakind2 = 1 THEN (stat.a).stanumbers2
            WHEN (stat.a).stakind3 = 1 THEN (stat.a).stanumbers3
            WHEN (stat.a).stakind4 = 1 THEN (stat.a).stanumbers4
            WHEN (stat.a).stakind5 = 1 THEN (stat.a).stanumbers5
            ELSE NULL::real[]
        END AS most_common_freqs,
        CASE
            WHEN (stat.a).stakind1 = 2 THEN (stat.a).stavalues1
            WHEN (stat.a).stakind2 = 2 THEN (stat.a).stavalues2
            WHEN (stat.a).stakind3 = 2 THEN (stat.a).stavalues3
            WHEN (stat.a).stakind4 = 2 THEN (stat.a).stavalues4
            WHEN (stat.a).stakind5 = 2 THEN (stat.a).stavalues5
            ELSE NULL::anyarray
        END AS histogram_bounds,
        CASE
            WHEN (stat.a).stakind1 = 3 THEN (stat.a).stanumbers1[1]
            WHEN (stat.a).stakind2 = 3 THEN (stat.a).stanumbers2[1]
            WHEN (stat.a).stakind3 = 3 THEN (stat.a).stanumbers3[1]
            WHEN (stat.a).stakind4 = 3 THEN (stat.a).stanumbers4[1]
            WHEN (stat.a).stakind5 = 3 THEN (stat.a).stanumbers5[1]
            ELSE NULL::real
        END AS correlation,
        CASE
            WHEN (stat.a).stakind1 = 4 THEN (stat.a).stavalues1
            WHEN (stat.a).stakind2 = 4 THEN (stat.a).stavalues2
            WHEN (stat.a).stakind3 = 4 THEN (stat.a).stavalues3
            WHEN (stat.a).stakind4 = 4 THEN (stat.a).stavalues4
            WHEN (stat.a).stakind5 = 4 THEN (stat.a).stavalues5
            ELSE NULL::anyarray
        END AS most_common_elems,
        CASE
            WHEN (stat.a).stakind1 = 4 THEN (stat.a).stanumbers1
            WHEN (stat.a).stakind2 = 4 THEN (stat.a).stanumbers2
            WHEN (stat.a).stakind3 = 4 THEN (stat.a).stanumbers3
            WHEN (stat.a).stakind4 = 4 THEN (stat.a).stanumbers4
            WHEN (stat.a).stakind5 = 4 THEN (stat.a).stanumbers5
            ELSE NULL::real[]
        END AS most_common_elem_freqs,
        CASE
            WHEN (stat.a).stakind1 = 5 THEN (stat.a).stanumbers1
            WHEN (stat.a).stakind2 = 5 THEN (stat.a).stanumbers2
            WHEN (stat.a).stakind3 = 5 THEN (stat.a).stanumbers3
            WHEN (stat.a).stakind4 = 5 THEN (stat.a).stanumbers4
            WHEN (stat.a).stakind5 = 5 THEN (stat.a).stanumbers5
            ELSE NULL::real[]
        END AS elem_count_histogram
   FROM pg_statistic_ext s
     JOIN pg_class c ON c.oid = s.stxrelid
     LEFT JOIN pg_statistic_ext_data sd ON s.oid = sd.stxoid
     LEFT JOIN pg_namespace cn ON cn.oid = c.relnamespace
     LEFT JOIN pg_namespace sn ON sn.oid = s.stxnamespace
     JOIN LATERAL ( SELECT unnest(pg_get_statisticsobjdef_expressions(s.oid)) AS expr,
            unnest(sd.stxdexpr) AS a) stat ON stat.expr IS NOT NULL
  WHERE pg_has_role(c.relowner, 'USAGE'::text) AND (c.relrowsecurity = false OR NOT row_security_active(c.oid));
;

CREATE VIEW pg_catalog.pg_tables 
as 
 SELECT n.nspname AS schemaname,
    c.relname AS tablename,
    pg_get_userbyid(c.relowner) AS tableowner,
    t.spcname AS tablespace,
    c.relhasindex AS hasindexes,
    c.relhasrules AS hasrules,
    c.relhastriggers AS hastriggers,
    c.relrowsecurity AS rowsecurity
   FROM pg_class c
     LEFT JOIN pg_namespace n ON n.oid = c.relnamespace
     LEFT JOIN pg_tablespace t ON t.oid = c.reltablespace
  WHERE c.relkind = ANY (ARRAY['r'::"char", 'p'::"char"]);
;

CREATE VIEW pg_catalog.pg_timezone_abbrevs 
as 
 SELECT abbrev,
    utc_offset,
    is_dst
   FROM pg_timezone_abbrevs() pg_timezone_abbrevs(abbrev, utc_offset, is_dst);
;

CREATE VIEW pg_catalog.pg_timezone_names 
as 
 SELECT name,
    abbrev,
    utc_offset,
    is_dst
   FROM pg_timezone_names() pg_timezone_names(name, abbrev, utc_offset, is_dst);
;

CREATE VIEW pg_catalog.pg_user_mappings 
as 
 SELECT u.oid AS umid,
    s.oid AS srvid,
    s.srvname,
    u.umuser,
        CASE
            WHEN u.umuser = 0::oid THEN 'public'::name
            ELSE a.rolname
        END AS usename,
        CASE
            WHEN u.umuser <> 0::oid AND a.rolname = CURRENT_USER AND (pg_has_role(s.srvowner, 'USAGE'::text) OR has_server_privilege(s.oid, 'USAGE'::text)) OR u.umuser = 0::oid AND pg_has_role(s.srvowner, 'USAGE'::text) OR ( SELECT pg_authid.rolsuper
               FROM pg_authid
              WHERE pg_authid.rolname = CURRENT_USER) THEN u.umoptions
            ELSE NULL::text[]
        END AS umoptions
   FROM pg_user_mapping u
     JOIN pg_foreign_server s ON u.umserver = s.oid
     LEFT JOIN pg_authid a ON a.oid = u.umuser;
;

CREATE VIEW pg_catalog.pg_views 
as 
 SELECT n.nspname AS schemaname,
    c.relname AS viewname,
    pg_get_userbyid(c.relowner) AS viewowner,
    pg_get_viewdef(c.oid) AS definition
   FROM pg_class c
     LEFT JOIN pg_namespace n ON n.oid = c.relnamespace
  WHERE c.relkind = 'v'::"char";
;

CREATE VIEW pg_catalog.pg_wait_events 
as 
 SELECT type,
    name,
    description
   FROM pg_get_wait_events() pg_get_wait_events(type, name, description);
;

CREATE VIEW information_schema._pg_user_mappings 
as 
 SELECT um.oid,
    um.umoptions,
    um.umuser,
    COALESCE(u.rolname, 'PUBLIC'::name)::information_schema.sql_identifier AS authorization_identifier,
    s.foreign_server_catalog,
    s.foreign_server_name,
    s.authorization_identifier AS srvowner
   FROM pg_user_mapping um
     LEFT JOIN pg_authid u ON u.oid = um.umuser,
    information_schema._pg_foreign_servers s
  WHERE s.oid = um.umserver;
;

CREATE VIEW information_schema.administrable_role_authorizations 
as 
 SELECT grantee,
    role_name,
    is_grantable
   FROM information_schema.applicable_roles
  WHERE is_grantable::text = 'YES'::text;
;

CREATE VIEW information_schema.column_options 
as 
 SELECT current_database()::information_schema.sql_identifier AS table_catalog,
    nspname::information_schema.sql_identifier AS table_schema,
    relname::information_schema.sql_identifier AS table_name,
    attname::information_schema.sql_identifier AS column_name,
    (pg_options_to_table(attfdwoptions)).option_name::information_schema.sql_identifier AS option_name,
    (pg_options_to_table(attfdwoptions)).option_value::information_schema.character_data AS option_value
   FROM information_schema._pg_foreign_table_columns c;
;

CREATE VIEW information_schema.data_type_privileges 
as 
 SELECT current_database()::information_schema.sql_identifier AS object_catalog,
    objschema AS object_schema,
    objname AS object_name,
    objtype::information_schema.character_data AS object_type,
    objdtdid AS dtd_identifier
   FROM ( SELECT attributes.udt_schema,
            attributes.udt_name,
            'USER-DEFINED TYPE'::text AS text,
            attributes.dtd_identifier
           FROM information_schema.attributes
        UNION ALL
         SELECT columns.table_schema,
            columns.table_name,
            'TABLE'::text AS text,
            columns.dtd_identifier
           FROM information_schema.columns
        UNION ALL
         SELECT domains.domain_schema,
            domains.domain_name,
            'DOMAIN'::text AS text,
            domains.dtd_identifier
           FROM information_schema.domains
        UNION ALL
         SELECT parameters.specific_schema,
            parameters.specific_name,
            'ROUTINE'::text AS text,
            parameters.dtd_identifier
           FROM information_schema.parameters
        UNION ALL
         SELECT routines.specific_schema,
            routines.specific_name,
            'ROUTINE'::text AS text,
            routines.dtd_identifier
           FROM information_schema.routines) x(objschema, objname, objtype, objdtdid);
;

CREATE VIEW information_schema.foreign_data_wrapper_options 
as 
 SELECT foreign_data_wrapper_catalog,
    foreign_data_wrapper_name,
    (pg_options_to_table(fdwoptions)).option_name::information_schema.sql_identifier AS option_name,
    (pg_options_to_table(fdwoptions)).option_value::information_schema.character_data AS option_value
   FROM information_schema._pg_foreign_data_wrappers w;
;

CREATE VIEW information_schema.foreign_data_wrappers 
as 
 SELECT foreign_data_wrapper_catalog,
    foreign_data_wrapper_name,
    authorization_identifier,
    NULL::character varying::information_schema.character_data AS library_name,
    foreign_data_wrapper_language
   FROM information_schema._pg_foreign_data_wrappers w;
;

CREATE VIEW information_schema.foreign_server_options 
as 
 SELECT foreign_server_catalog,
    foreign_server_name,
    (pg_options_to_table(srvoptions)).option_name::information_schema.sql_identifier AS option_name,
    (pg_options_to_table(srvoptions)).option_value::information_schema.character_data AS option_value
   FROM information_schema._pg_foreign_servers s;
;

CREATE VIEW information_schema.foreign_servers 
as 
 SELECT foreign_server_catalog,
    foreign_server_name,
    foreign_data_wrapper_catalog,
    foreign_data_wrapper_name,
    foreign_server_type,
    foreign_server_version,
    authorization_identifier
   FROM information_schema._pg_foreign_servers;
;

CREATE VIEW information_schema.foreign_table_options 
as 
 SELECT foreign_table_catalog,
    foreign_table_schema,
    foreign_table_name,
    (pg_options_to_table(ftoptions)).option_name::information_schema.sql_identifier AS option_name,
    (pg_options_to_table(ftoptions)).option_value::information_schema.character_data AS option_value
   FROM information_schema._pg_foreign_tables t;
;

CREATE VIEW information_schema.foreign_tables 
as 
 SELECT foreign_table_catalog,
    foreign_table_schema,
    foreign_table_name,
    foreign_server_catalog,
    foreign_server_name
   FROM information_schema._pg_foreign_tables;
;

CREATE VIEW information_schema.role_column_grants 
as 
 SELECT grantor,
    grantee,
    table_catalog,
    table_schema,
    table_name,
    column_name,
    privilege_type,
    is_grantable
   FROM information_schema.column_privileges
  WHERE (grantor::name IN ( SELECT enabled_roles.role_name
           FROM information_schema.enabled_roles)) OR (grantee::name IN ( SELECT enabled_roles.role_name
           FROM information_schema.enabled_roles));
;

CREATE VIEW information_schema.role_routine_grants 
as 
 SELECT grantor,
    grantee,
    specific_catalog,
    specific_schema,
    specific_name,
    routine_catalog,
    routine_schema,
    routine_name,
    privilege_type,
    is_grantable
   FROM information_schema.routine_privileges
  WHERE (grantor::name IN ( SELECT enabled_roles.role_name
           FROM information_schema.enabled_roles)) OR (grantee::name IN ( SELECT enabled_roles.role_name
           FROM information_schema.enabled_roles));
;

CREATE VIEW information_schema.role_table_grants 
as 
 SELECT grantor,
    grantee,
    table_catalog,
    table_schema,
    table_name,
    privilege_type,
    is_grantable,
    with_hierarchy
   FROM information_schema.table_privileges
  WHERE (grantor::name IN ( SELECT enabled_roles.role_name
           FROM information_schema.enabled_roles)) OR (grantee::name IN ( SELECT enabled_roles.role_name
           FROM information_schema.enabled_roles));
;

CREATE VIEW information_schema.role_udt_grants 
as 
 SELECT grantor,
    grantee,
    udt_catalog,
    udt_schema,
    udt_name,
    privilege_type,
    is_grantable
   FROM information_schema.udt_privileges
  WHERE (grantor::name IN ( SELECT enabled_roles.role_name
           FROM information_schema.enabled_roles)) OR (grantee::name IN ( SELECT enabled_roles.role_name
           FROM information_schema.enabled_roles));
;

CREATE VIEW information_schema.role_usage_grants 
as 
 SELECT grantor,
    grantee,
    object_catalog,
    object_schema,
    object_name,
    object_type,
    privilege_type,
    is_grantable
   FROM information_schema.usage_privileges
  WHERE (grantor::name IN ( SELECT enabled_roles.role_name
           FROM information_schema.enabled_roles)) OR (grantee::name IN ( SELECT enabled_roles.role_name
           FROM information_schema.enabled_roles));
;

CREATE VIEW pg_catalog.pg_stat_replication_slots 
as 
 SELECT s.slot_name,
    s.spill_txns,
    s.spill_count,
    s.spill_bytes,
    s.stream_txns,
    s.stream_count,
    s.stream_bytes,
    s.total_txns,
    s.total_bytes,
    s.stats_reset
   FROM pg_replication_slots r,
    LATERAL pg_stat_get_replication_slot(r.slot_name::text) s(slot_name, spill_txns, spill_count, spill_bytes, stream_txns, stream_count, stream_bytes, total_txns, total_bytes, stats_reset)
  WHERE r.datoid IS NOT NULL;
;

CREATE VIEW pg_catalog.pg_stat_sys_indexes 
as 
 SELECT relid,
    indexrelid,
    schemaname,
    relname,
    indexrelname,
    idx_scan,
    last_idx_scan,
    idx_tup_read,
    idx_tup_fetch
   FROM pg_stat_all_indexes
  WHERE (schemaname = ANY (ARRAY['pg_catalog'::name, 'information_schema'::name])) OR schemaname ~ '^pg_toast'::text;
;

CREATE VIEW pg_catalog.pg_stat_sys_tables 
as 
 SELECT relid,
    schemaname,
    relname,
    seq_scan,
    last_seq_scan,
    seq_tup_read,
    idx_scan,
    last_idx_scan,
    idx_tup_fetch,
    n_tup_ins,
    n_tup_upd,
    n_tup_del,
    n_tup_hot_upd,
    n_tup_newpage_upd,
    n_live_tup,
    n_dead_tup,
    n_mod_since_analyze,
    n_ins_since_vacuum,
    last_vacuum,
    last_autovacuum,
    last_analyze,
    last_autoanalyze,
    vacuum_count,
    autovacuum_count,
    analyze_count,
    autoanalyze_count
   FROM pg_stat_all_tables
  WHERE (schemaname = ANY (ARRAY['pg_catalog'::name, 'information_schema'::name])) OR schemaname ~ '^pg_toast'::text;
;

CREATE VIEW pg_catalog.pg_stat_user_indexes 
as 
 SELECT relid,
    indexrelid,
    schemaname,
    relname,
    indexrelname,
    idx_scan,
    last_idx_scan,
    idx_tup_read,
    idx_tup_fetch
   FROM pg_stat_all_indexes
  WHERE (schemaname <> ALL (ARRAY['pg_catalog'::name, 'information_schema'::name])) AND schemaname !~ '^pg_toast'::text;
;

CREATE VIEW pg_catalog.pg_stat_user_tables 
as 
 SELECT relid,
    schemaname,
    relname,
    seq_scan,
    last_seq_scan,
    seq_tup_read,
    idx_scan,
    last_idx_scan,
    idx_tup_fetch,
    n_tup_ins,
    n_tup_upd,
    n_tup_del,
    n_tup_hot_upd,
    n_tup_newpage_upd,
    n_live_tup,
    n_dead_tup,
    n_mod_since_analyze,
    n_ins_since_vacuum,
    last_vacuum,
    last_autovacuum,
    last_analyze,
    last_autoanalyze,
    vacuum_count,
    autovacuum_count,
    analyze_count,
    autoanalyze_count
   FROM pg_stat_all_tables
  WHERE (schemaname <> ALL (ARRAY['pg_catalog'::name, 'information_schema'::name])) AND schemaname !~ '^pg_toast'::text;
;

CREATE VIEW pg_catalog.pg_stat_xact_sys_tables 
as 
 SELECT relid,
    schemaname,
    relname,
    seq_scan,
    seq_tup_read,
    idx_scan,
    idx_tup_fetch,
    n_tup_ins,
    n_tup_upd,
    n_tup_del,
    n_tup_hot_upd,
    n_tup_newpage_upd
   FROM pg_stat_xact_all_tables
  WHERE (schemaname = ANY (ARRAY['pg_catalog'::name, 'information_schema'::name])) OR schemaname ~ '^pg_toast'::text;
;

CREATE VIEW pg_catalog.pg_stat_xact_user_tables 
as 
 SELECT relid,
    schemaname,
    relname,
    seq_scan,
    seq_tup_read,
    idx_scan,
    idx_tup_fetch,
    n_tup_ins,
    n_tup_upd,
    n_tup_del,
    n_tup_hot_upd,
    n_tup_newpage_upd
   FROM pg_stat_xact_all_tables
  WHERE (schemaname <> ALL (ARRAY['pg_catalog'::name, 'information_schema'::name])) AND schemaname !~ '^pg_toast'::text;
;

CREATE VIEW pg_catalog.pg_statio_sys_indexes 
as 
 SELECT relid,
    indexrelid,
    schemaname,
    relname,
    indexrelname,
    idx_blks_read,
    idx_blks_hit
   FROM pg_statio_all_indexes
  WHERE (schemaname = ANY (ARRAY['pg_catalog'::name, 'information_schema'::name])) OR schemaname ~ '^pg_toast'::text;
;

CREATE VIEW pg_catalog.pg_statio_sys_sequences 
as 
 SELECT relid,
    schemaname,
    relname,
    blks_read,
    blks_hit
   FROM pg_statio_all_sequences
  WHERE (schemaname = ANY (ARRAY['pg_catalog'::name, 'information_schema'::name])) OR schemaname ~ '^pg_toast'::text;
;

CREATE VIEW pg_catalog.pg_statio_sys_tables 
as 
 SELECT relid,
    schemaname,
    relname,
    heap_blks_read,
    heap_blks_hit,
    idx_blks_read,
    idx_blks_hit,
    toast_blks_read,
    toast_blks_hit,
    tidx_blks_read,
    tidx_blks_hit
   FROM pg_statio_all_tables
  WHERE (schemaname = ANY (ARRAY['pg_catalog'::name, 'information_schema'::name])) OR schemaname ~ '^pg_toast'::text;
;

CREATE VIEW pg_catalog.pg_statio_user_indexes 
as 
 SELECT relid,
    indexrelid,
    schemaname,
    relname,
    indexrelname,
    idx_blks_read,
    idx_blks_hit
   FROM pg_statio_all_indexes
  WHERE (schemaname <> ALL (ARRAY['pg_catalog'::name, 'information_schema'::name])) AND schemaname !~ '^pg_toast'::text;
;

CREATE VIEW pg_catalog.pg_statio_user_sequences 
as 
 SELECT relid,
    schemaname,
    relname,
    blks_read,
    blks_hit
   FROM pg_statio_all_sequences
  WHERE (schemaname <> ALL (ARRAY['pg_catalog'::name, 'information_schema'::name])) AND schemaname !~ '^pg_toast'::text;
;

CREATE VIEW pg_catalog.pg_statio_user_tables 
as 
 SELECT relid,
    schemaname,
    relname,
    heap_blks_read,
    heap_blks_hit,
    idx_blks_read,
    idx_blks_hit,
    toast_blks_read,
    toast_blks_hit,
    tidx_blks_read,
    tidx_blks_hit
   FROM pg_statio_all_tables
  WHERE (schemaname <> ALL (ARRAY['pg_catalog'::name, 'information_schema'::name])) AND schemaname !~ '^pg_toast'::text;
;

CREATE VIEW pg_catalog.pg_user 
as 
 SELECT usename,
    usesysid,
    usecreatedb,
    usesuper,
    userepl,
    usebypassrls,
    '********'::text AS passwd,
    valuntil,
    useconfig
   FROM pg_shadow;
;

CREATE VIEW information_schema.element_types 
as 
 SELECT current_database()::information_schema.sql_identifier AS object_catalog,
    n.nspname::information_schema.sql_identifier AS object_schema,
    x.objname AS object_name,
    x.objtype::information_schema.character_data AS object_type,
    x.objdtdid::information_schema.sql_identifier AS collection_type_identifier,
        CASE
            WHEN nbt.nspname = 'pg_catalog'::name THEN format_type(bt.oid, NULL::integer)
            ELSE 'USER-DEFINED'::text
        END::information_schema.character_data AS data_type,
    NULL::integer::information_schema.cardinal_number AS character_maximum_length,
    NULL::integer::information_schema.cardinal_number AS character_octet_length,
    NULL::name::information_schema.sql_identifier AS character_set_catalog,
    NULL::name::information_schema.sql_identifier AS character_set_schema,
    NULL::name::information_schema.sql_identifier AS character_set_name,
        CASE
            WHEN nco.nspname IS NOT NULL THEN current_database()
            ELSE NULL::name
        END::information_schema.sql_identifier AS collation_catalog,
    nco.nspname::information_schema.sql_identifier AS collation_schema,
    co.collname::information_schema.sql_identifier AS collation_name,
    NULL::integer::information_schema.cardinal_number AS numeric_precision,
    NULL::integer::information_schema.cardinal_number AS numeric_precision_radix,
    NULL::integer::information_schema.cardinal_number AS numeric_scale,
    NULL::integer::information_schema.cardinal_number AS datetime_precision,
    NULL::character varying::information_schema.character_data AS interval_type,
    NULL::integer::information_schema.cardinal_number AS interval_precision,
    current_database()::information_schema.sql_identifier AS udt_catalog,
    nbt.nspname::information_schema.sql_identifier AS udt_schema,
    bt.typname::information_schema.sql_identifier AS udt_name,
    NULL::name::information_schema.sql_identifier AS scope_catalog,
    NULL::name::information_schema.sql_identifier AS scope_schema,
    NULL::name::information_schema.sql_identifier AS scope_name,
    NULL::integer::information_schema.cardinal_number AS maximum_cardinality,
    ('a'::text || x.objdtdid::text)::information_schema.sql_identifier AS dtd_identifier
   FROM pg_namespace n,
    pg_type at,
    pg_namespace nbt,
    pg_type bt,
    ( SELECT c.relnamespace,
            c.relname::information_schema.sql_identifier AS relname,
                CASE
                    WHEN c.relkind = 'c'::"char" THEN 'USER-DEFINED TYPE'::text
                    ELSE 'TABLE'::text
                END AS "case",
            a.attnum,
            a.atttypid,
            a.attcollation
           FROM pg_class c,
            pg_attribute a
          WHERE c.oid = a.attrelid AND (c.relkind = ANY (ARRAY['r'::"char", 'v'::"char", 'f'::"char", 'c'::"char", 'p'::"char"])) AND a.attnum > 0 AND NOT a.attisdropped
        UNION ALL
         SELECT t.typnamespace,
            t.typname::information_schema.sql_identifier AS typname,
            'DOMAIN'::text AS text,
            1,
            t.typbasetype,
            t.typcollation
           FROM pg_type t
          WHERE t.typtype = 'd'::"char"
        UNION ALL
         SELECT ss.pronamespace,
            nameconcatoid(ss.proname, ss.oid)::information_schema.sql_identifier AS nameconcatoid,
            'ROUTINE'::text AS text,
            (ss.x).n AS n,
            (ss.x).x AS x,
            0
           FROM ( SELECT p.pronamespace,
                    p.proname,
                    p.oid,
                    information_schema._pg_expandarray(COALESCE(p.proallargtypes, p.proargtypes::oid[])) AS x
                   FROM pg_proc p) ss
        UNION ALL
         SELECT p.pronamespace,
            nameconcatoid(p.proname, p.oid)::information_schema.sql_identifier AS nameconcatoid,
            'ROUTINE'::text AS text,
            0,
            p.prorettype,
            0
           FROM pg_proc p) x(objschema, objname, objtype, objdtdid, objtypeid, objcollation)
     LEFT JOIN (pg_collation co
     JOIN pg_namespace nco ON co.collnamespace = nco.oid) ON x.objcollation = co.oid AND (nco.nspname <> 'pg_catalog'::name OR co.collname <> 'default'::name)
  WHERE n.oid = x.objschema AND at.oid = x.objtypeid AND at.typelem <> 0::oid AND at.typlen = '-1'::integer AND at.typelem = bt.oid AND nbt.oid = bt.typnamespace AND ((n.nspname, x.objname::name, x.objtype, x.objdtdid::information_schema.sql_identifier::name) IN ( SELECT data_type_privileges.object_schema,
            data_type_privileges.object_name,
            data_type_privileges.object_type,
            data_type_privileges.dtd_identifier
           FROM information_schema.data_type_privileges));
;

CREATE VIEW information_schema.user_mapping_options 
as 
 SELECT um.authorization_identifier,
    um.foreign_server_catalog,
    um.foreign_server_name,
    opts.option_name::information_schema.sql_identifier AS option_name,
        CASE
            WHEN um.umuser <> 0::oid AND um.authorization_identifier::name = CURRENT_USER OR um.umuser = 0::oid AND pg_has_role(um.srvowner::name, 'USAGE'::text) OR ( SELECT pg_authid.rolsuper
               FROM pg_authid
              WHERE pg_authid.rolname = CURRENT_USER) THEN opts.option_value
            ELSE NULL::text
        END::information_schema.character_data AS option_value
   FROM information_schema._pg_user_mappings um,
    LATERAL pg_options_to_table(um.umoptions) opts(option_name, option_value);
;

CREATE VIEW information_schema.user_mappings 
as 
 SELECT authorization_identifier,
    foreign_server_catalog,
    foreign_server_name
   FROM information_schema._pg_user_mappings;
;

