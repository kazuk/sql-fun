# Query access matrix

| Relation | Column | Access purpose | Rust queries (source; statement) |
| --- | --- | --- | --- |
| public.e2e_items | — | DeleteRows | `delete_item` (&lt;unknown&gt;:34; 1) |
| public.e2e_items | — | InsertRows | `insert_item` (&lt;unknown&gt;:13; 1) |
| public.e2e_items | — | ReadRows | `cte_item` (&lt;unknown&gt;:55; 1); `delete_item` (&lt;unknown&gt;:34; 1); `find_item` (&lt;unknown&gt;:41; 1); `insert_item` (&lt;unknown&gt;:13; 1); `joined_item` (&lt;unknown&gt;:48; 1); `nested_item` (&lt;unknown&gt;:62; 1); `select_item` (&lt;unknown&gt;:20; 1); `update_item` (&lt;unknown&gt;:27; 1) |
| public.e2e_items | — | UpdateRows | `update_item` (&lt;unknown&gt;:27; 1) |
| public.e2e_items | id | DeleteFilter | `delete_item` (&lt;unknown&gt;:34; 1) |
| public.e2e_items | id | DeleteReturning | `delete_item` (&lt;unknown&gt;:34; 1) |
| public.e2e_items | id | InsertReturning | `insert_item` (&lt;unknown&gt;:13; 1) |
| public.e2e_items | id | InsertTarget | `insert_item` (&lt;unknown&gt;:13; 1) |
| public.e2e_items | id | SelectFilter | `cte_item` (&lt;unknown&gt;:55; 1); `find_item` (&lt;unknown&gt;:41; 1); `joined_item` (&lt;unknown&gt;:48; 1); `nested_item` (&lt;unknown&gt;:62; 1); `select_item` (&lt;unknown&gt;:20; 1) |
| public.e2e_items | id | SelectJoin | `joined_item` (&lt;unknown&gt;:48; 1) |
| public.e2e_items | id | SelectOutput | `cte_item` (&lt;unknown&gt;:55; 1); `find_item` (&lt;unknown&gt;:41; 1); `joined_item` (&lt;unknown&gt;:48; 1); `nested_item` (&lt;unknown&gt;:62; 1); `select_item` (&lt;unknown&gt;:20; 1) |
| public.e2e_items | id | SelectSource | `cte_item` (&lt;unknown&gt;:55; 1) |
| public.e2e_items | id | UpdateFilter | `update_item` (&lt;unknown&gt;:27; 1) |
| public.e2e_items | id | UpdateInput | `update_item` (&lt;unknown&gt;:27; 1) |
| public.e2e_items | id | UpdateReturning | `update_item` (&lt;unknown&gt;:27; 1) |
| public.e2e_items | id | UpdateTarget | `update_item` (&lt;unknown&gt;:27; 1) |
| public.e2e_items | name | DeleteReturning | `delete_item` (&lt;unknown&gt;:34; 1) |
| public.e2e_items | name | InsertReturning | `insert_item` (&lt;unknown&gt;:13; 1) |
| public.e2e_items | name | InsertTarget | `insert_item` (&lt;unknown&gt;:13; 1) |
| public.e2e_items | name | SelectOutput | `cte_item` (&lt;unknown&gt;:55; 1); `find_item` (&lt;unknown&gt;:41; 1); `joined_item` (&lt;unknown&gt;:48; 1); `nested_item` (&lt;unknown&gt;:62; 1); `select_item` (&lt;unknown&gt;:20; 1) |
| public.e2e_items | name | SelectSource | `cte_item` (&lt;unknown&gt;:55; 1) |
| public.e2e_items | name | UpdateReturning | `update_item` (&lt;unknown&gt;:27; 1) |
| public.e2e_items | name | UpdateTarget | `update_item` (&lt;unknown&gt;:27; 1) |
| public.e2e_item_view | — | ReadRows | `view_item` (&lt;unknown&gt;:69; 1) |
| public.e2e_item_view | id | SelectFilter | `view_item` (&lt;unknown&gt;:69; 1) |
| public.e2e_item_view | id | SelectOutput | `view_item` (&lt;unknown&gt;:69; 1) |
| public.e2e_item_view | name | SelectOutput | `view_item` (&lt;unknown&gt;:69; 1) |
