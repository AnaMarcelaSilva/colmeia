//! Tarefas de exemplo, fixas no código. Na versão final vêm do núcleo.

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Coluna {
    Backlog,
    Trabalhando,
    AguardandoVoce,
    Revisao,
    Concluido,
}

impl Coluna {
    pub const TODAS: [Coluna; 5] = [
        Coluna::Backlog,
        Coluna::Trabalhando,
        Coluna::AguardandoVoce,
        Coluna::Revisao,
        Coluna::Concluido,
    ];

    pub fn nome(self) -> &'static str {
        match self {
            Coluna::Backlog => "Backlog",
            Coluna::Trabalhando => "Agente trabalhando",
            Coluna::AguardandoVoce => "Aguardando você",
            Coluna::Revisao => "Revisão",
            Coluna::Concluido => "Concluído",
        }
    }
}

pub struct Tarefa {
    pub id: u32,
    pub projeto: &'static str,
    pub titulo: String,
    pub coluna: Coluna,
    pub branch: &'static str,
    /// Terminais do núcleo que trabalham nesta tarefa.
    pub agentes: Vec<usize>,
    /// Por que a tarefa está esperando você.
    pub motivo: Option<&'static str>,
    /// Erro de um agente ou integração, e se você já abriu a tarefa depois dele.
    pub erro: Option<&'static str>,
    pub erro_visto: bool,
}

pub const PROJETOS: [&str; 3] = ["loja-web", "api-pedidos", "estudos-rust"];

pub const BRANCHES: [&str; 5] = ["dev", "main", "feature/pedidos", "feature/clientes", "hotfix/desconto"];

pub fn gerar(quantidade: usize) -> Vec<Tarefa> {
    let mut tarefas = vec![
        Tarefa {
            id: 101,
            projeto: "loja-web",
            titulo: "Nova tela de pedidos".into(),
            coluna: Coluna::Trabalhando,
            branch: "feature/pedidos",
            agentes: vec![0, 1, 2, 3],
            motivo: None,
            erro: None,
            erro_visto: false,
        },
        Tarefa {
            id: 102,
            projeto: "loja-web",
            titulo: "Corrigir desconto do cupom no checkout".into(),
            coluna: Coluna::AguardandoVoce,
            branch: "hotfix/desconto",
            agentes: vec![4, 5],
            motivo: Some("pede aprovação: push na dev"),
            erro: None,
            erro_visto: false,
        },
        Tarefa {
            id: 103,
            projeto: "api-pedidos",
            titulo: "Cadastro de clientes".into(),
            coluna: Coluna::Trabalhando,
            branch: "feature/clientes",
            agentes: vec![6, 7],
            motivo: None,
            erro: None,
            erro_visto: false,
        },
        Tarefa {
            id: 104,
            projeto: "api-pedidos",
            titulo: "Revisar integração de estoque".into(),
            coluna: Coluna::Revisao,
            branch: "dev",
            agentes: vec![8, 9],
            motivo: None,
            erro: None,
            erro_visto: false,
        },
    ];

    let verbos = ["Ajustar", "Criar", "Corrigir", "Revisar", "Documentar", "Migrar", "Otimizar", "Testar"];
    let objetos = [
        "tela de estoque",
        "relatório de vendas",
        "impressão de etiquetas",
        "cadastro de produtos",
        "sincronização do ERP",
        "login com SSO",
        "filtro de pedidos",
        "exportação para CSV",
        "fechamento de caixa",
        "notificações",
    ];
    // Tarefas sem agente ficam só nas colunas que não dependem de um.
    let colunas = [Coluna::Backlog, Coluna::Backlog, Coluna::Revisao, Coluna::Concluido, Coluna::Concluido];
    for i in tarefas.len()..quantidade {
        tarefas.push(Tarefa {
            id: 105 + i as u32,
            projeto: PROJETOS[i % PROJETOS.len()],
            titulo: format!("{} {}", verbos[i % verbos.len()], objetos[(i / verbos.len()) % objetos.len()]),
            coluna: colunas[(i * 7) % colunas.len()],
            branch: BRANCHES[(i * 3 + i / 5) % BRANCHES.len()],
            agentes: Vec::new(),
            motivo: None,
            erro: None,
            erro_visto: false,
        });
    }
    tarefas
}
