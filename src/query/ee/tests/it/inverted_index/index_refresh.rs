// Copyright 2023 Databend Cloud
//
// Licensed under the Elastic License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     https://www.elastic.co/licensing/elastic-license
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use databend_common_base::base::tokio;
use databend_common_catalog::table::Table;
use databend_common_catalog::table::TableExt;
use databend_common_exception::Result;
use databend_common_expression::DataSchema;
use databend_common_meta_app::schema::CreateOption;
use databend_common_meta_app::schema::CreateTableIndexReq;
use databend_common_sql::plans::RefreshTableIndexPlan;
use databend_common_storages_fuse::io::read::load_inverted_index_info;
use databend_common_storages_fuse::io::read::InvertedIndexReader;
use databend_common_storages_fuse::FuseTable;
use databend_common_storages_fuse::TableContext;
use databend_enterprise_inverted_index::get_inverted_index_handler;
use databend_enterprise_query::test_kits::context::EESetup;
use databend_query::interpreters::Interpreter;
use databend_query::interpreters::RefreshTableIndexInterpreter;
use databend_query::test_kits::append_string_sample_data;
use databend_query::test_kits::*;

#[tokio::test(flavor = "multi_thread")]
async fn test_fuse_do_refresh_inverted_index() -> Result<()> {
    let fixture = TestFixture::setup_with_custom(EESetup::new()).await?;

    fixture
        .default_session()
        .get_settings()
        .set_data_retention_time_in_days(0)?;
    fixture.create_default_database().await?;
    fixture.create_string_table().await?;

    let number_of_block = 2;
    append_string_sample_data(number_of_block, &fixture).await?;

    let table = fixture.latest_default_table().await?;

    let handler = get_inverted_index_handler();

    let ctx = fixture.new_query_ctx().await?;
    let catalog = ctx.get_catalog(&fixture.default_catalog_name()).await?;
    let table_id = table.get_id();
    let index_name = "idx1".to_string();
    let req = CreateTableIndexReq {
        create_option: CreateOption::Create,
        table_id,
        name: index_name.clone(),
        column_ids: vec![0, 1],
        sync_creation: true,
    };

    let res = handler.do_create_table_index(catalog.clone(), req).await;
    assert!(res.is_ok());

    let refresh_index_plan = RefreshTableIndexPlan {
        catalog: fixture.default_catalog_name(),
        database: fixture.default_db_name(),
        table: fixture.default_table_name(),
        index_name: index_name.clone(),
        segment_locs: None,
        need_lock: true,
    };
    let interpreter = RefreshTableIndexInterpreter::try_create(ctx.clone(), refresh_index_plan)?;
    let _ = interpreter.execute(ctx.clone()).await?;

    let new_table = table.refresh(ctx.as_ref()).await?;
    let new_fuse_table = FuseTable::do_create(new_table.get_table_info().clone())?;
    let table_schema = new_fuse_table.schema();

    // get index location from new table snapshot
    let new_snapshot = new_fuse_table.read_table_snapshot().await?;
    assert!(new_snapshot.is_some());
    let new_snapshot = new_snapshot.unwrap();
    assert!(new_snapshot.index_info_locations.is_some());
    let index_info_locations = new_snapshot.index_info_locations.clone().unwrap();
    let index_info_loc = index_info_locations.get(&index_name);
    assert!(index_info_loc.is_some());
    let index_info =
        load_inverted_index_info(new_fuse_table.get_operator(), index_info_loc).await?;
    assert!(index_info.is_some());
    let index_info = index_info.unwrap();
    assert_eq!(index_info.indexes.len(), 1);

    let dal = new_fuse_table.get_operator_ref();
    let schema = DataSchema::from(table_schema);
    let query_columns = vec!["title".to_string(), "content".to_string()];
    let index_reader =
        InvertedIndexReader::try_create(dal.clone(), &schema, &query_columns, &index_info.indexes)
            .await?;

    let query = "rust";
    let segment_map = index_reader.do_filter(query)?;
    assert_eq!(segment_map.len(), 1);
    let segment_row_id_scores: Vec<_> = segment_map.into_values().collect();
    assert_eq!(segment_row_id_scores.len(), 1);
    let row_id_scores = &segment_row_id_scores[0];
    assert!(row_id_scores.is_some());
    let row_id_scores = row_id_scores.as_ref().unwrap();
    assert_eq!(row_id_scores.len(), 2);
    assert_eq!(row_id_scores[0].0, 0);
    assert_eq!(row_id_scores[1].0, 1);

    let query = "java";
    let segment_map = index_reader.do_filter(query)?;
    assert_eq!(segment_map.len(), 1);
    let segment_row_id_scores: Vec<_> = segment_map.into_values().collect();
    assert_eq!(segment_row_id_scores.len(), 1);
    let row_id_scores = &segment_row_id_scores[0];
    assert!(row_id_scores.is_some());
    let row_id_scores = row_id_scores.as_ref().unwrap();
    assert_eq!(row_id_scores.len(), 1);
    assert_eq!(row_id_scores[0].0, 2);

    let query = "data";
    let segment_map = index_reader.do_filter(query)?;
    assert_eq!(segment_map.len(), 1);
    let segment_row_id_scores: Vec<_> = segment_map.into_values().collect();
    assert_eq!(segment_row_id_scores.len(), 1);
    let row_id_scores = &segment_row_id_scores[0];
    assert!(row_id_scores.is_some());
    let row_id_scores = row_id_scores.as_ref().unwrap();
    assert_eq!(row_id_scores.len(), 3);
    assert_eq!(row_id_scores[0].0, 1);
    assert_eq!(row_id_scores[1].0, 4);
    assert_eq!(row_id_scores[2].0, 5);

    Ok(())
}
