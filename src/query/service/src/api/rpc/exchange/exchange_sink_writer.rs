// Copyright 2023 Datafuse Labs.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::sync::Arc;

use common_exception::ErrorCode;
use common_exception::Result;
use common_expression::DataBlock;
use common_pipeline_core::pipe::PipeItem;
use common_pipeline_core::processors::port::InputPort;
use common_pipeline_core::processors::processor::ProcessorPtr;
use common_pipeline_sinks::AsyncSink;
use common_pipeline_sinks::AsyncSinker;

use crate::api::rpc::exchange::serde::exchange_serializer::ExchangeSerializeMeta;
use crate::api::rpc::flight_client::FlightExchange;

pub struct ExchangeWriterSink {
    exchange: FlightExchange,
}

impl ExchangeWriterSink {
    pub fn create(input: Arc<InputPort>, flight_exchange: FlightExchange) -> ProcessorPtr {
        AsyncSinker::create(input, ExchangeWriterSink {
            exchange: flight_exchange,
        })
    }
}

#[async_trait::async_trait]
impl AsyncSink for ExchangeWriterSink {
    const NAME: &'static str = "ExchangeWriterSink";

    async fn on_finish(&mut self) -> Result<()> {
        self.exchange.close_output();
        Ok(())
    }

    #[async_trait::unboxed_simple]
    async fn consume(&mut self, mut data_block: DataBlock) -> Result<()> {
        let packet = match data_block.take_meta() {
            None => Err(ErrorCode::Internal(
                "ExchangeWriterSink only recv ExchangeSerializeMeta.",
            )),
            Some(mut block_meta) => match block_meta
                .as_mut_any()
                .downcast_mut::<ExchangeSerializeMeta>()
            {
                None => Err(ErrorCode::Internal(
                    "ExchangeWriterSink only recv ExchangeSerializeMeta.",
                )),
                Some(block_meta) => Ok(block_meta.packet.take().unwrap()),
            },
        }?;

        self.exchange.send(packet).await?;
        Ok(())
    }
}

pub fn create_writer_item(exchange: FlightExchange) -> PipeItem {
    let input = InputPort::create();
    PipeItem::create(
        ExchangeWriterSink::create(input.clone(), exchange),
        vec![input],
        vec![],
    )
}

pub fn create_writer_items(exchanges: Vec<FlightExchange>) -> Vec<PipeItem> {
    exchanges.into_iter().map(create_writer_item).collect()
}