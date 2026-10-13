//! Contadores de "conhecimento" que cada lado mantém sobre o outro.

/// `budgetKnowledge` no bundle.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BudgetKnowledge {
    /// Sobe a cada mudança local.
    pub current_device_knowledge: i64,
    /// Até onde o servidor já recebeu as mudanças deste aparelho.
    pub server_knowledge_of_device: i64,
    /// Até onde este aparelho já recebeu as mudanças do servidor.
    pub device_knowledge_of_server: i64,
    pub schema_version_of_knowledge: i64,
    /// `queueCalculationsForServerEntities`: 1 quando entidades do servidor
    /// colidem com as enviadas e os cálculos precisam ser refeitos.
    pub queue_calculations_for_server_entities: i64,
}

impl BudgetKnowledge {
    /// `hasBudgetChangesToSendToServer`.
    pub fn has_changes_to_send(&self) -> bool {
        self.current_device_knowledge > self.server_knowledge_of_device
    }
}
