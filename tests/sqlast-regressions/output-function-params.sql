CREATE FUNCTION public.output520_table() RETURNS TABLE ("Mixed Name" integer, label text) LANGUAGE sql AS 'SELECT 1, NULL::text';
CREATE FUNCTION public.output520_inout(INOUT "Mixed Name" integer, OUT label text) LANGUAGE sql AS 'SELECT $1, NULL::text';
CREATE FUNCTION public.output520_scalar(INOUT value integer) LANGUAGE sql AS 'SELECT $1';
