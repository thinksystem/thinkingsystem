// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2024 Jonathan Lee
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License version 3
// as published by the Free Software Foundation.
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the GNU Affero General Public License for more details.
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see https://www.gnu.org/licenses/.

use crate::messaging::types::{
    EdgeLabel, GraphEdge, GraphNode, Message, MessageDestination, NodeType,
};
use anyhow::{Context, Result};
use chrono::Utc;
use std::collections::HashMap;
use surrealdb::engine::remote::ws::{Client, Ws};
use surrealdb::opt::auth::Root;
use surrealdb::Surreal;

pub struct MessagingApp {
    db: Surreal<Client>,
}

impl MessagingApp {
    pub async fn new(db_url: &str, username: &str, password: &str) -> Result<Self> {
        let db = Surreal::new::<Ws>(db_url)
            .await
            .context("Failed to connect to SurrealDB")?;

        db.signin(Root { username, password })
            .await
            .context("Failed to authenticate with SurrealDB")?;

        db.use_ns("messaging")
            .use_db("chat")
            .await
            .context("Failed to select namespace and database")?;

        Ok(MessagingApp { db })
    }

    pub async fn send_message(
        &self,
        from: String,
        to: MessageDestination,
        content: String,
    ) -> Result<Message> {
        let message = Message::new(from, to, content);
        let mid = message.mid.clone();

        let created: Option<Message> = self
            .db
            .create(("messages", mid.as_str()))
            .content(message.clone())
            .await
            .context("Failed to create message in database")?;

        let msg = created.context("Message creation returned None")?;

        let node_id = format!("message:{mid}");
        let mut properties = HashMap::new();
        properties.insert("mid".to_string(), serde_json::Value::String(mid.clone()));
        properties.insert(
            "content".to_string(),
            serde_json::Value::String(msg.content.clone()),
        );
        properties.insert(
            "sender".to_string(),
            serde_json::Value::String(msg.sender.clone()),
        );

        let message_node = GraphNode {
            id: node_id.clone(),
            node_type: NodeType::Message.as_str().to_string(),
            properties,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let _: Option<GraphNode> = self
            .db
            .create(("nodes", node_id.as_str()))
            .content(message_node)
            .await
            .context("Failed to create graph node")?;

        let _bridge_result = self
            .db
            .query("CREATE message_nodes SET message_id = $msg_id, node_id = $node_id")
            .bind(("msg_id", mid.clone()))
            .bind(("node_id", node_id))
            .await
            .context("Failed to create message-node bridge")?;

        Ok(msg)
    }
    pub async fn get_conversation_between(
        &self,
        user1: String,
        user2: String,
        limit: usize,
    ) -> Result<Vec<Message>> {
        let query = format!(
            "SELECT * FROM messages
             WHERE (meta.from = $user1 AND meta.to = $user2) OR
                   (meta.from = $user2 AND meta.to = $user1)
             ORDER BY meta.ts DESC
             LIMIT {limit}"
        );

        let messages: Vec<Message> = self
            .db
            .query(&query)
            .bind(("user1", user1))
            .bind(("user2", user2))
            .await
            .context("Failed to query conversation messages")?
            .take(0)
            .context("Failed to extract conversation messages from query result")?;

        Ok(messages)
    }

    pub async fn get_message_by_id(&self, mid: String) -> Result<Option<Message>> {
        let message: Option<Message> = self
            .db
            .select(("messages", mid))
            .await
            .context("Failed to query message by ID")?;

        Ok(message)
    }

    pub async fn delete_message(&self, mid: String) -> Result<bool> {
        let result: Option<Message> = self
            .db
            .delete(("messages", mid))
            .await
            .context("Failed to delete message")?;

        Ok(result.is_some())
    }

    pub async fn mark_message_delivered(&self, mid: String) -> Result<bool> {
        let updated: Option<Message> = self
            .db
            .query("UPDATE messages SET meta.status = 'delivered' WHERE mid = $mid RETURN AFTER")
            .bind(("mid", mid))
            .await
            .context("Failed to mark message as delivered")?
            .take(0)
            .context("Failed to extract updated message")?;

        Ok(updated.is_some())
    }

    pub async fn search_messages(&self, query: String, user_id: String) -> Result<Vec<Message>> {
        let messages: Vec<Message> = self.db
            .query("SELECT * FROM messages WHERE (meta.from = $user OR meta.to = $user OR meta.to CONTAINS $user) AND content CONTAINS $query")
            .bind(("user", user_id))
            .bind(("query", query))
            .await
            .context("Failed to search messages")?
            .take(0)
            .context("Failed to extract search results")?;

        Ok(messages)
    }

    pub async fn get_recent_messages(&self, user_id: String, limit: usize) -> Result<Vec<Message>> {
        let messages: Vec<Message> = self.db
            .query(format!("SELECT * FROM messages WHERE meta.from = $user OR meta.to = $user OR meta.to CONTAINS $user ORDER BY timestamp DESC LIMIT {limit}"))
            .bind(("user", user_id))
            .await
            .context("Failed to get recent messages")?
            .take(0)
            .context("Failed to extract recent messages")?;

        Ok(messages)
    }

    pub async fn store_message(&self, message: &Message) -> Result<()> {
        let mid = message.mid.clone();
        let message_clone = message.clone();

        let _created: Option<Message> = self
            .db
            .create(("messages", mid.as_str()))
            .content(message_clone)
            .await
            .context("Failed to store message")?;

        Ok(())
    }
    pub async fn update_message_status(&self, id: String, status: String) -> Result<bool> {
        let updated: Option<Message> = self
            .db
            .query("UPDATE messages SET meta.status = $status WHERE mid = $id RETURN AFTER")
            .bind(("id", id))
            .bind(("status", status))
            .await
            .context("Failed to update message status")?
            .take(0)
            .context("Failed to extract updated message")?;

        Ok(updated.is_some())
    }

    pub async fn create_graph_node(&self, node: GraphNode) -> Result<GraphNode> {
        let created: Option<GraphNode> = self
            .db
            .create(("nodes", node.id.as_str()))
            .content(node)
            .await
            .context("Failed to create graph node")?;

        created.context("Graph node creation returned None")
    }

    pub async fn create_graph_edge(&self, edge: GraphEdge) -> Result<GraphEdge> {
        let created: Option<GraphEdge> = self
            .db
            .create(("edges", edge.id.as_str()))
            .content(edge)
            .await
            .context("Failed to create graph edge")?;

        created.context("Graph edge creation returned None")
    }

    pub async fn get_conversation_messages(&self, conversation_id: &str) -> Result<Vec<Message>> {
        let messages: Vec<Message> = self
            .db
            .query(
                "
                SELECT VALUE m.* FROM messages m
                WHERE m.mid IN (
                    SELECT VALUE e.from_node FROM edges e
                    WHERE e.label = 'belongs_to'
                    AND e.to_node = $conv_id
                    AND e.from_node LIKE 'message:%'
                )
                ORDER BY m.timestamp ASC
            ",
            )
            .bind(("conv_id", format!("conversation:{conversation_id}")))
            .await
            .context("Failed to get conversation messages")?
            .take(0)
            .context("Failed to extract conversation messages")?;

        Ok(messages)
    }

    pub async fn get_message_replies(&self, message_id: &str) -> Result<Vec<Message>> {
        let messages: Vec<Message> = self
            .db
            .query(
                "
                SELECT VALUE m.* FROM messages m
                WHERE m.mid IN (
                    SELECT VALUE SPLIT(e.from_node, ':')[1] FROM edges e
                    WHERE e.label = 'replies_to'
                    AND e.to_node = $msg_node
                )
                ORDER BY m.timestamp ASC
            ",
            )
            .bind(("msg_node", format!("message:{message_id}")))
            .await
            .context("Failed to get message replies")?
            .take(0)
            .context("Failed to extract message replies")?;

        Ok(messages)
    }

    pub async fn get_message_mentions(&self, message_id: &str) -> Result<Vec<String>> {
        let users: Vec<String> = self
            .db
            .query(
                "
                SELECT VALUE SPLIT(e.to_node, ':')[1] FROM edges e
                WHERE e.label = 'mentions'
                AND e.from_node = $msg_node
            ",
            )
            .bind(("msg_node", format!("message:{message_id}")))
            .await
            .context("Failed to get message mentions")?
            .take(0)
            .context("Failed to extract message mentions")?;

        Ok(users)
    }

    pub async fn create_reply_relationship(
        &self,
        reply_message_id: &str,
        original_message_id: &str,
        reply_type: &str,
    ) -> Result<()> {
        let edge_id = format!("reply_{reply_message_id}_{original_message_id}");
        let mut properties = HashMap::new();
        properties.insert(
            "reply_type".to_string(),
            serde_json::Value::String(reply_type.to_string()),
        );
        properties.insert(
            "timestamp".to_string(),
            serde_json::Value::String(Utc::now().to_rfc3339()),
        );

        let edge = GraphEdge {
            id: edge_id,
            from_node: format!("message:{reply_message_id}"),
            to_node: format!("message:{original_message_id}"),
            label: EdgeLabel::RepliesTo.as_str().to_string(),
            properties,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        self.create_graph_edge(edge).await?;
        Ok(())
    }

    pub async fn add_user_to_conversation(
        &self,
        user_id: &str,
        conversation_id: &str,
        role: &str,
    ) -> Result<()> {
        let edge_id = format!("participation_{user_id}_{conversation_id}");
        let mut properties = HashMap::new();
        properties.insert(
            "role".to_string(),
            serde_json::Value::String(role.to_string()),
        );
        properties.insert(
            "joined_at".to_string(),
            serde_json::Value::String(Utc::now().to_rfc3339()),
        );

        let edge = GraphEdge {
            id: edge_id,
            from_node: format!("user:{user_id}"),
            to_node: format!("conversation:{conversation_id}"),
            label: EdgeLabel::ParticipatesIn.as_str().to_string(),
            properties,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        self.create_graph_edge(edge).await?;
        Ok(())
    }
}
