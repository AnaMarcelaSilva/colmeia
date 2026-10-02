//! Modelo que a tela desenha. No uso normal ele vem do núcleo (veja `api`);
//! no modo demonstração, dos exemplos fixos daqui.

use crate::api;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Coluna {
    Backlog,
    Trabalhando,
    AguardandoVoce,
    Revisao,
    Concluido,
}

impl Coluna {
    pub const TODAS: [Coluna; 5] = [Coluna::Backlog, Coluna::Trabalhando, Coluna::AguardandoVoce, Coluna::Revisao, Coluna::Concluido];

    pub fn nome(self) -> &'static str {
        match self {
            Coluna::Backlog => "Backlog",
            Coluna::Trabalhando => "Agente trabalhando",
            Coluna::AguardandoVoce => "Aguardando você",
            Coluna::Revisao => "Revisão",
            Coluna::Concluido => "Concluído",
        }
    }

    /// Nome usado na API do núcleo.
    pub fn chave(self) -> &'static str {
        match self {
            Coluna::Backlog => "backlog",
            Coluna::Trabalhando => "trabalhando",
            Coluna::AguardandoVoce => "aguardando",
            Coluna::Revisao => "revisao",
            Coluna::Concluido => "concluido",
        }
    }

    pub fn da_chave(chave: &str) -> Coluna {
        Coluna::TODAS.into_iter().find(|c| c.chave() == chave).unwrap_or(Coluna::Backlog)
    }
}

/// Projeto como a tela precisa dele.
#[derive(Clone, Debug)]
pub struct Projeto {
    pub id: i64,
    pub nome: String,
    pub workspace: String,
    pub caminho: String,
    pub branch_padrao: String,
}

impl From<api::Projeto> for Projeto {
    fn from(p: api::Projeto) -> Self {
        Projeto { id: p.id, nome: p.nome, workspace: p.workspace, caminho: p.caminho, branch_padrao: p.branch_padrao }
    }
}

pub struct Tarefa {
    pub id: i64,
    pub projeto_id: i64,
    /// Nome do projeto, mostrado no cartão quando o quadro junta vários projetos.
    pub projeto: String,
    pub titulo: String,
    pub coluna: Coluna,
    pub branch: String,
    /// Terminais do núcleo que trabalham nesta tarefa.
    pub agentes: Vec<usize>,
    /// Por que a tarefa está esperando você.
    pub motivo: Option<&'static str>,
    /// Erro de um agente ou integração, e se você já abriu a tarefa depois dele.
    pub erro: Option<&'static str>,
    pub erro_visto: bool,
}

impl Tarefa {
    pub fn da_api(t: api::Tarefa, projeto: &str) -> Tarefa {
        Tarefa {
            id: t.id,
            projeto_id: t.projeto_id,
            projeto: projeto.to_string(),
            titulo: t.titulo,
            coluna: Coluna::da_chave(&t.coluna),
            branch: t.branch,
            agentes: Vec::new(),
            motivo: None,
            erro: None,
            erro_visto: false,
        }
    }
}

// Modo demonstração: três projetos e tarefas de exemplo, com agentes de mentira.

pub const BRANCHES_DEMO: [&str; 5] = ["dev", "main", "feature/pedidos", "feature/clientes", "hotfix/desconto"];

pub fn projetos_demo() -> Vec<Projeto> {
    ["loja-web", "api-pedidos", "estudos-rust"]
        .into_iter()
        .enumerate()
        .map(|(i, nome)| Projeto { id: i as i64 + 1, nome: nome.into(), workspace: "Empresa X".into(), caminho: String::new(), branch_padrao: "main".into() })
        .collect()
}

pub fn gerar_demo(quantidade: usize) -> Vec<Tarefa> {
    let projetos = projetos_demo();
    let nova = |id: i64, projeto: usize, titulo: &str, coluna: Coluna, branch: &str, agentes: Vec<usize>, motivo: Option<&'static str>| Tarefa {
        id,
        projeto_id: projetos[projeto].id,
        projeto: projetos[projeto].nome.clone(),
        titulo: titulo.into(),
        coluna,
        branch: branch.into(),
        agentes,
        motivo,
        erro: None,
        erro_visto: false,
    };
    let mut tarefas = vec![
        nova(101, 0, "Nova tela de pedidos", Coluna::Trabalhando, "feature/pedidos", vec![0, 1, 2, 3], None),
        nova(102, 0, "Corrigir desconto do cupom no checkout", Coluna::AguardandoVoce, "hotfix/desconto", vec![4, 5], Some("pede aprovação: push na dev")),
        nova(103, 1, "Cadastro de clientes", Coluna::Trabalhando, "feature/clientes", vec![6, 7], None),
        nova(104, 1, "Revisar integração de estoque", Coluna::Revisao, "dev", vec![8, 9], None),
    ];

    let verbos = ["Ajustar", "Criar", "Corrigir", "Revisar", "Documentar", "Migrar", "Otimizar", "Testar"];
    let objetos = [
        "tela de estoque",
        "relatório de vendas",
        "impressão de etiquetas",
        "cadastro de produtos",
        "sincronização de estoque",
        "login com SSO",
        "filtro de pedidos",
        "exportação para CSV",
        "fechamento de caixa",
        "notificações",
    ];
    // Tarefas sem agente ficam só nas colunas que não dependem de um.
    let colunas = [Coluna::Backlog, Coluna::Backlog, Coluna::Revisao, Coluna::Concluido, Coluna::Concluido];
    for i in tarefas.len()..quantidade {
        let titulo = format!("{} {}", verbos[i % verbos.len()], objetos[(i / verbos.len()) % objetos.len()]);
        let branch = BRANCHES_DEMO[(i * 3 + i / 5) % BRANCHES_DEMO.len()];
        tarefas.push(nova(105 + i as i64, i % projetos.len(), &titulo, colunas[(i * 7) % colunas.len()], branch, Vec::new(), None));
    }
    tarefas
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn colunas_vao_e_voltam_pela_chave_da_api() {
        for c in Coluna::TODAS {
            assert_eq!(Coluna::da_chave(c.chave()), c);
        }
        // Uma coluna desconhecida vinda do núcleo cai no Backlog, em vez de sumir.
        assert_eq!(Coluna::da_chave("inventada"), Coluna::Backlog);
    }

    #[test]
    fn demo_tem_tarefas_em_todos_os_projetos() {
        let tarefas = gerar_demo(50);
        assert_eq!(tarefas.len(), 50);
        for p in projetos_demo() {
            assert!(tarefas.iter().any(|t| t.projeto_id == p.id && t.projeto == p.nome));
        }
    }
}
