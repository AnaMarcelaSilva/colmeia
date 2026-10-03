//! Modelo que a tela desenha. No uso normal ele vem do núcleo (veja `api`);
//! no modo demonstração, dos exemplos fixos daqui.

use crate::api;
use crate::tema::EstadoVisual;

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
    /// O workspace: o id (para a lousa dele) e o nome.
    pub workspace_id: i64,
    pub workspace: String,
    pub caminho: String,
    /// Pasta de trabalho sem git: sem branches nem cópias isoladas.
    pub sem_git: bool,
    pub branch_padrao: String,
}

impl From<api::Projeto> for Projeto {
    fn from(p: api::Projeto) -> Self {
        Projeto {
            id: p.id,
            nome: p.nome,
            workspace_id: p.workspace_id,
            workspace: p.workspace,
            caminho: p.caminho,
            sem_git: p.tipo == "pasta",
            branch_padrao: p.branch_padrao,
        }
    }
}

/// Estado de um agente rodando, como o núcleo vê pela saída do terminal.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum EstadoAgente {
    #[default]
    Trabalhando,
    Ocioso,
    Aguardando,
}

impl EstadoAgente {
    pub fn da_chave(chave: &str) -> EstadoAgente {
        match chave {
            "ocioso" => EstadoAgente::Ocioso,
            "aguardando" => EstadoAgente::Aguardando,
            _ => EstadoAgente::Trabalhando,
        }
    }
}

/// Motivo fixo do núcleo para um agente esperar você.
pub const PEDE_APROVACAO: &str = "pede aprovação";

/// Um agente da tarefa como a tela mostra. `id` também é a chave do terminal.
#[derive(Clone, Debug)]
pub struct AgenteTela {
    pub id: i64,
    pub ferramenta: String,
    pub papel: String,
    /// O terminal do agente está rodando (vem dos eventos agente.iniciou e agente.terminou).
    pub ativo: bool,
    pub estado: EstadoAgente,
    /// Por que espera você (da lista fixa do núcleo).
    pub motivo: String,
    /// Hora local da última mudança de estado, "14:32".
    pub desde: String,
    /// O mesmo momento em RFC 3339 (UTC), para comparar com outros momentos do núcleo.
    pub desde_em: String,
    /// Como terminou, se parou.
    pub fim: Option<api::Fim>,
    /// Você já viu o erro dele (abriu a tarefa); a abelha para de insistir.
    pub erro_visto: bool,
}

impl AgenteTela {
    pub fn da_api(a: &api::Agente) -> AgenteTela {
        AgenteTela {
            id: a.id,
            ferramenta: a.ferramenta.clone(),
            papel: a.papel.clone(),
            ativo: a.ativo,
            estado: EstadoAgente::da_chave(&a.estado),
            motivo: a.motivo.clone(),
            desde: a.desde_hora.clone(),
            desde_em: a.desde.clone(),
            fim: if a.ativo { None } else { a.ultimo_fim.clone() },
            // Erros de antes de a tela abrir entram como já vistos: a faixa fica, a abelha não insiste.
            erro_visto: true,
        }
    }

    /// Nome da ferramenta para mostrar.
    pub fn nome(&self) -> String {
        match self.ferramenta.as_str() {
            "claude" => "Claude Code".into(),
            "codex" => "Codex".into(),
            "gemini" => "Gemini CLI".into(),
            "opencode" => "OpenCode".into(),
            "shell" => "Terminal".into(),
            outro => outro.into(),
        }
    }

    /// "Claude Code (dev)".
    pub fn nome_com_papel(&self) -> String {
        format!("{} ({})", self.nome(), self.papel)
    }

    pub fn visual(&self) -> EstadoVisual {
        if self.ativo {
            return match self.estado {
                EstadoAgente::Trabalhando => EstadoVisual::Trabalhando,
                EstadoAgente::Ocioso => EstadoVisual::Parado,
                EstadoAgente::Aguardando if self.motivo == PEDE_APROVACAO => EstadoVisual::PedeAprovacao,
                EstadoAgente::Aguardando => EstadoVisual::SuaVez,
            };
        }
        match &self.fim {
            Some(f) if f.erro => EstadoVisual::Erro,
            Some(f) if f.motivo == "interrompido" => EstadoVisual::Interrompido,
            _ => EstadoVisual::Terminou,
        }
    }

    /// O texto curto do estado, com hora fixa (nada relativo, que pediria redesenho).
    pub fn texto_estado(&self) -> String {
        let hora = |h: &str| if h.is_empty() { String::new() } else { format!(" às {h}") };
        let desde = |h: &str| if h.is_empty() { String::new() } else { format!(" · desde {h}") };
        match self.visual() {
            EstadoVisual::Trabalhando => "Trabalhando".into(),
            EstadoVisual::PedeAprovacao => format!("Parece pedir aprovação{}", desde(&self.desde)),
            EstadoVisual::SuaVez => format!("Sua vez{}", desde(&self.desde)),
            EstadoVisual::Parado if self.desde.is_empty() => "Parado".into(),
            EstadoVisual::Parado => format!("Parado desde {}", self.desde),
            EstadoVisual::Erro => {
                let f = self.fim.as_ref();
                format!("Parou com erro (código {}){}", f.map_or(0, |f| f.codigo), hora(f.map_or("", |f| &f.hora)))
            }
            EstadoVisual::Interrompido => format!("Interrompido{}", hora(self.fim.as_ref().map_or("", |f| &f.hora))),
            _ => match &self.fim {
                Some(f) if self.ferramenta == "shell" && f.codigo != 0 && f.motivo == "terminou" => {
                    format!("Terminal encerrado (código {}){}", f.codigo, hora(&f.hora))
                }
                Some(f) => format!("Terminou{}", hora(&f.hora)),
                None => "Parado".into(),
            },
        }
    }

    /// O estado em duas partes, curto, para o cartão do quadro: o começo pode
    /// ser cortado, a hora (o fim) não. "Pede aprovação" + " · 14:32".
    pub fn estado_curto(&self) -> (String, String) {
        let hora = |h: &str| if h.is_empty() { String::new() } else { format!(" · {h}") };
        match self.visual() {
            EstadoVisual::PedeAprovacao => ("Pede aprovação".into(), hora(&self.desde)),
            EstadoVisual::SuaVez => ("Sua vez".into(), hora(&self.desde)),
            EstadoVisual::Parado => ("Parado".into(), hora(&self.desde)),
            _ => {
                let texto = self.texto_estado();
                match texto.rsplit_once(" às ") {
                    Some((inicio, h)) => (inicio.to_string(), format!(" às {h}")),
                    None => (texto, String::new()),
                }
            }
        }
    }

    /// Erro que você ainda não viu.
    pub fn erro_novo(&self) -> bool {
        self.visual() == EstadoVisual::Erro && !self.erro_visto
    }
}

pub struct Tarefa {
    pub id: i64,
    pub projeto_id: i64,
    /// Nome do projeto, mostrado no cartão quando o quadro junta vários projetos.
    pub projeto: String,
    pub titulo: String,
    pub coluna: Coluna,
    /// A última mudança de coluna foi do núcleo, não sua.
    pub coluna_auto: bool,
    pub branch: String,
    /// Onde os agentes trabalham: a pasta do projeto ou a cópia isolada da tarefa.
    pub pasta: String,
    pub em_copia: bool,
    /// Agentes da tarefa; cada um tem um terminal no núcleo enquanto roda.
    pub agentes: Vec<AgenteTela>,
    /// Por que a tarefa está esperando você (na demonstração, fixo; no uso
    /// normal, calculado dos agentes por `derivar`).
    pub motivo: Option<String>,
    /// Hora desde quando espera, "14:32".
    pub motivo_desde: String,
    /// Só a ação ("pede aprovação"), para o cartão que o núcleo moveu.
    pub motivo_acao: String,
    /// Erro de um agente, e se você já abriu a tarefa depois dele.
    pub erro: Option<String>,
    pub erro_visto: bool,
}

impl Tarefa {
    pub fn da_api(t: api::Tarefa, projeto: &Projeto) -> Tarefa {
        let mut nova = Tarefa {
            id: t.id,
            projeto_id: t.projeto_id,
            projeto: projeto.nome.clone(),
            titulo: String::new(),
            coluna: Coluna::Backlog,
            coluna_auto: false,
            branch: String::new(),
            pasta: String::new(),
            em_copia: false,
            agentes: Vec::new(),
            motivo: None,
            motivo_desde: String::new(),
            motivo_acao: String::new(),
            erro: None,
            erro_visto: false,
        };
        nova.atualizar(t, projeto);
        nova
    }

    /// Troca os campos vindos do núcleo, mantendo os agentes.
    fn atualizar(&mut self, t: api::Tarefa, projeto: &Projeto) {
        let em_copia = t.local == "copia" && !t.copia.is_empty();
        self.projeto = projeto.nome.clone();
        self.titulo = t.titulo;
        self.coluna = Coluna::da_chave(&t.coluna);
        self.coluna_auto = t.coluna_auto;
        self.branch = t.branch;
        self.pasta = if em_copia { t.copia } else { projeto.caminho.clone() };
        self.em_copia = em_copia;
    }

    /// Calcula o aviso de espera e o de erro a partir dos agentes.
    pub fn derivar(&mut self) {
        let esperando = self.agentes.iter().find(|a| a.ativo && a.estado == EstadoAgente::Aguardando);
        let acao = esperando.map(|a| if a.motivo == PEDE_APROVACAO { "pede aprovação" } else { "espera sua resposta" });
        self.motivo = esperando.zip(acao).map(|(a, acao)| format!("{} {acao}", a.nome_com_papel()));
        self.motivo_acao = acao.unwrap_or_default().to_string();
        self.motivo_desde = esperando.map(|a| a.desde.clone()).unwrap_or_default();
        let com_erro: Vec<&AgenteTela> = self.agentes.iter().filter(|a| a.visual() == EstadoVisual::Erro).collect();
        self.erro = com_erro.first().map(|a| format!("{} parou (código {})", a.nome_com_papel(), a.fim.as_ref().map_or(0, |f| f.codigo)));
        self.erro_visto = com_erro.iter().all(|a| a.erro_visto);
    }

    /// Abrir a tarefa conta como ver os erros dela.
    pub fn marcar_erros_vistos(&mut self) {
        for a in &mut self.agentes {
            a.erro_visto = true;
        }
        self.erro_visto = true;
    }
}

/// Tudo o que o quadro mostra de um perfil. Muda só por `carregar` (o retrato
/// inteiro) e por `aplicar` (uma mensagem de eventos).
#[derive(Default)]
pub struct Modelo {
    pub projetos: Vec<Projeto>,
    pub tarefas: Vec<Tarefa>,
    /// Pedidos ao agente: os abertos e os fechados há pouco (um por id).
    pub pedidos: Vec<api::Pedido>,
    /// Tarefas com o navegador da Colmeia aberto.
    pub navegadores: std::collections::HashSet<i64>,
}

/// O que a tela principal precisa fazer depois de aplicar um evento.
#[derive(Debug, PartialEq)]
pub enum Efeito {
    /// O terminal do agente começou a rodar: ligar a tela a ele.
    Conectar(i64),
    /// O agente saiu: soltar o terminal.
    Soltar(i64),
    /// Uma tarefa chegou em Concluído: a abelha comemora.
    Concluiu { tarefa: i64, projeto: i64 },
    /// Um agente passou a precisar de você (esperando ou com erro).
    Atencao { tarefa: i64, agente: i64, texto: String, erro: bool },
    /// O núcleo pediu para refazer o retrato do quadro.
    Recarregar,
    /// O agente respondeu um pedido (a nota e as capturas já chegaram).
    PedidoRespondido { tarefa: i64, agente: i64, titulo: String },
}

/// Mensagem do WebSocket de eventos do núcleo. Tipos desconhecidos são
/// ignorados: um núcleo mais novo pode mandar coisas que esta tela não conhece.
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(tag = "tipo")]
pub enum Evento {
    #[serde(rename = "ola")]
    Ola {
        seq: u64,
        #[serde(default)]
        aviso: Option<String>,
    },
    #[serde(rename = "recarregar")]
    Recarregar,
    #[serde(rename = "projeto.criado")]
    ProjetoCriado { seq: u64, projeto: api::Projeto },
    #[serde(rename = "projeto.removido")]
    ProjetoRemovido { seq: u64, projeto_id: i64 },
    #[serde(rename = "tarefa.criada")]
    TarefaCriada { seq: u64, tarefa: api::Tarefa },
    #[serde(rename = "tarefa.atualizada")]
    TarefaAtualizada { seq: u64, tarefa: api::Tarefa },
    #[serde(rename = "tarefa.removida")]
    TarefaRemovida { seq: u64, tarefa_id: i64 },
    #[serde(rename = "agente.criado")]
    AgenteCriado { seq: u64, agente: api::Agente },
    #[serde(rename = "agente.removido")]
    AgenteRemovido { seq: u64, agente_id: i64, tarefa_id: i64 },
    #[serde(rename = "agente.iniciou")]
    AgenteIniciou {
        seq: u64,
        agente_id: i64,
        tarefa_id: i64,
        desde_hora: String,
        #[serde(default)]
        desde: String,
    },
    #[serde(rename = "agente.terminou")]
    AgenteTerminou { seq: u64, agente_id: i64, tarefa_id: i64, fim: api::Fim },
    #[serde(rename = "agente.estado")]
    AgenteEstado {
        seq: u64,
        agente_id: i64,
        tarefa_id: i64,
        estado: String,
        #[serde(default)]
        motivo: String,
        #[serde(default)]
        desde_hora: String,
        #[serde(default)]
        desde: String,
    },
    #[serde(rename = "anexo.adicionado")]
    AnexoAdicionado {
        seq: u64,
        #[serde(default)]
        tarefa_id: i64,
        /// Imagem ou vídeo de uma lousa: não é novidade da linha do tempo.
        #[serde(default)]
        na_lousa: bool,
    },
    #[serde(rename = "anexo.removido")]
    AnexoRemovido {
        seq: u64,
        #[serde(default)]
        tarefa_id: i64,
    },
    #[serde(rename = "nota.atualizada")]
    NotaAtualizada {
        seq: u64,
        #[serde(default)]
        tarefa_id: i64,
        /// O agente que escreveu (pelas ferramentas da Colmeia); 0 é você.
        #[serde(default)]
        agente_id: i64,
    },
    /// Qualquer mudança de um pedido ao agente, com o pedido inteiro.
    #[serde(rename = "pedido.criado", alias = "pedido.entregue", alias = "pedido.respondido", alias = "pedido.cancelado", alias = "pedido.falhou")]
    PedidoMudou { seq: u64, pedido: api::Pedido },
    #[serde(rename = "navegador.aberto")]
    NavegadorAberto { seq: u64, tarefa_id: i64 },
    #[serde(rename = "navegador.fechado")]
    NavegadorFechado { seq: u64, tarefa_id: i64 },
    #[serde(rename = "navegador.captura", alias = "navegador.recusado")]
    NavegadorCaptura { seq: u64 },
    /// Mudou a lousa de um workspace ou de uma tarefa: os itens inteiros e
    /// os removidos. `agente_id` diz que foi o agente (e entra na linha do tempo).
    #[serde(rename = "lousa.mudou")]
    LousaMudou {
        seq: u64,
        lousa_id: i64,
        #[serde(default)]
        dono: api::DonoLousa,
        #[serde(default)]
        elementos: Vec<api::ElementoLousa>,
        #[serde(default)]
        removidos: Vec<i64>,
        #[serde(default)]
        agente_id: i64,
    },
    #[serde(other)]
    Desconhecido,
}

impl Evento {
    /// Número da mensagem (0 nas que não têm).
    pub fn seq(&self) -> u64 {
        match self {
            Evento::Ola { seq, .. }
            | Evento::ProjetoCriado { seq, .. }
            | Evento::ProjetoRemovido { seq, .. }
            | Evento::TarefaCriada { seq, .. }
            | Evento::TarefaAtualizada { seq, .. }
            | Evento::TarefaRemovida { seq, .. }
            | Evento::AgenteCriado { seq, .. }
            | Evento::AgenteRemovido { seq, .. }
            | Evento::AgenteIniciou { seq, .. }
            | Evento::AgenteTerminou { seq, .. }
            | Evento::AgenteEstado { seq, .. }
            | Evento::AnexoAdicionado { seq, .. }
            | Evento::AnexoRemovido { seq, .. }
            | Evento::NotaAtualizada { seq, .. }
            | Evento::PedidoMudou { seq, .. }
            | Evento::NavegadorAberto { seq, .. }
            | Evento::NavegadorFechado { seq, .. }
            | Evento::NavegadorCaptura { seq }
            | Evento::LousaMudou { seq, .. } => *seq,
            Evento::Recarregar | Evento::Desconhecido => 0,
        }
    }

    /// Muda algo que a linha do tempo mostra (o estado de um agente não muda).
    pub fn entra_na_linha(&self) -> bool {
        match self {
            // Mexer na lousa não entra na linha do tempo; o que o agente acrescenta entra.
            Evento::LousaMudou { agente_id, .. } => *agente_id != 0,
            Evento::AnexoAdicionado { na_lousa: true, .. } => false,
            _ => !matches!(self, Evento::Ola { .. } | Evento::Recarregar | Evento::AgenteEstado { .. } | Evento::Desconhecido),
        }
    }
}

impl Modelo {
    /// Troca tudo pelo retrato do quadro vindo do núcleo.
    pub fn carregar(&mut self, q: api::Quadro) {
        self.projetos = q.projetos.into_iter().map(Projeto::from).collect();
        let mut tarefas: Vec<Tarefa> = Vec::with_capacity(q.tarefas.len());
        for t in q.tarefas {
            if let Some(p) = self.projetos.iter().find(|p| p.id == t.projeto_id) {
                tarefas.push(Tarefa::da_api(t, p));
            }
        }
        for a in &q.agentes {
            if let Some(t) = tarefas.iter_mut().find(|t| t.id == a.tarefa_id) {
                t.agentes.push(AgenteTela::da_api(a));
            }
        }
        for t in &mut tarefas {
            t.derivar();
        }
        self.tarefas = tarefas;
        self.pedidos = q.pedidos;
        self.navegadores = q.navegadores.into_iter().collect();
    }

    /// O pedido que o cartão da tarefa mostra: o aberto mais antigo (o que
    /// está com o agente ou é o próximo) ou, sem aberto, o último fechado.
    pub fn pedido_da_tarefa(&self, tarefa: i64) -> Option<&api::Pedido> {
        let da_tarefa = || self.pedidos.iter().filter(move |p| p.tarefa_id == tarefa);
        da_tarefa().find(|p| p.aberto()).or_else(|| da_tarefa().filter(|p| p.estado != "cancelado").max_by_key(|p| p.id))
    }

    fn agente(&mut self, tarefa: i64, agente: i64) -> Option<(&mut Tarefa, usize)> {
        let t = self.tarefas.iter_mut().find(|t| t.id == tarefa)?;
        let i = t.agentes.iter().position(|a| a.id == agente)?;
        Some((t, i))
    }

    /// Aplica uma mensagem de eventos. Sem efeito colateral fora do modelo: o
    /// que a tela precisa fazer volta como `Efeito`. Aplicar a mesma mensagem
    /// duas vezes dá o mesmo resultado (cria ou atualiza pelo id; remover o
    /// que não existe não faz nada).
    pub fn aplicar(&mut self, evento: Evento) -> Vec<Efeito> {
        let mut efeitos = Vec::new();
        match evento {
            Evento::Ola { .. }
            | Evento::Desconhecido
            | Evento::AnexoAdicionado { .. }
            | Evento::AnexoRemovido { .. }
            | Evento::NotaAtualizada { .. }
            | Evento::NavegadorCaptura { .. }
            // A lousa aberta aplica (veja lousa::Lousa::aplicar).
            | Evento::LousaMudou { .. } => {}
            Evento::PedidoMudou { pedido, .. } => {
                let respondido = pedido.estado == "respondido";
                let (tarefa, agente) = (pedido.tarefa_id, pedido.agente_id);
                match self.pedidos.iter_mut().find(|p| p.id == pedido.id) {
                    Some(p) => {
                        let ja_sabia = p.estado == pedido.estado;
                        *p = pedido;
                        if ja_sabia {
                            return efeitos;
                        }
                    }
                    None => self.pedidos.push(pedido),
                }
                if respondido && let Some(t) = self.tarefas.iter().find(|t| t.id == tarefa) {
                    efeitos.push(Efeito::PedidoRespondido { tarefa, agente, titulo: t.titulo.clone() });
                }
            }
            Evento::NavegadorAberto { tarefa_id, .. } => {
                self.navegadores.insert(tarefa_id);
            }
            Evento::NavegadorFechado { tarefa_id, .. } => {
                self.navegadores.remove(&tarefa_id);
            }
            Evento::Recarregar => efeitos.push(Efeito::Recarregar),
            Evento::ProjetoCriado { projeto, .. } => {
                let projeto = Projeto::from(projeto);
                match self.projetos.iter_mut().find(|p| p.id == projeto.id) {
                    Some(p) => *p = projeto,
                    None => {
                        // Na mesma ordem do núcleo: workspace e nome.
                        let i = self.projetos.partition_point(|p| (&p.workspace, &p.nome) < (&projeto.workspace, &projeto.nome));
                        self.projetos.insert(i, projeto);
                    }
                }
            }
            Evento::ProjetoRemovido { projeto_id, .. } => {
                self.projetos.retain(|p| p.id != projeto_id);
                for t in self.tarefas.iter().filter(|t| t.projeto_id == projeto_id) {
                    efeitos.extend(t.agentes.iter().map(|a| Efeito::Soltar(a.id)));
                }
                self.tarefas.retain(|t| t.projeto_id != projeto_id);
            }
            Evento::TarefaCriada { tarefa, .. } | Evento::TarefaAtualizada { tarefa, .. } => {
                let Some(projeto) = self.projetos.iter().find(|p| p.id == tarefa.projeto_id) else {
                    return efeitos;
                };
                match self.tarefas.iter().position(|t| t.id == tarefa.id) {
                    Some(i) => {
                        let antes = self.tarefas[i].coluna;
                        let t = &mut self.tarefas[i];
                        t.atualizar(tarefa, projeto);
                        if t.coluna != antes {
                            if t.coluna == Coluna::Concluido {
                                efeitos.push(Efeito::Concluiu { tarefa: t.id, projeto: t.projeto_id });
                            }
                            // Mudou de coluna: vai para o fim dela, como no núcleo.
                            let t = self.tarefas.remove(i);
                            self.tarefas.push(t);
                        }
                    }
                    None => self.tarefas.push(Tarefa::da_api(tarefa, projeto)),
                }
            }
            Evento::TarefaRemovida { tarefa_id, .. } => {
                if let Some(t) = self.tarefas.iter().find(|t| t.id == tarefa_id) {
                    efeitos.extend(t.agentes.iter().map(|a| Efeito::Soltar(a.id)));
                }
                self.tarefas.retain(|t| t.id != tarefa_id);
                self.pedidos.retain(|p| p.tarefa_id != tarefa_id);
                self.navegadores.remove(&tarefa_id);
            }
            Evento::AgenteCriado { agente, .. } => {
                if let Some(t) = self.tarefas.iter_mut().find(|t| t.id == agente.tarefa_id) {
                    match t.agentes.iter_mut().find(|a| a.id == agente.id) {
                        // Já existe (a tela criou e já pôs): os dados vivos ficam.
                        Some(a) => {
                            a.ferramenta = agente.ferramenta;
                            a.papel = agente.papel;
                        }
                        None => t.agentes.push(AgenteTela::da_api(&agente)),
                    }
                    t.derivar();
                }
            }
            Evento::AgenteRemovido { agente_id, tarefa_id, .. } => {
                if let Some(t) = self.tarefas.iter_mut().find(|t| t.id == tarefa_id) {
                    t.agentes.retain(|a| a.id != agente_id);
                    t.derivar();
                }
                efeitos.push(Efeito::Soltar(agente_id));
            }
            Evento::AgenteIniciou { agente_id, tarefa_id, desde_hora, desde, .. } => {
                if let Some((t, i)) = self.agente(tarefa_id, agente_id) {
                    let a = &mut t.agentes[i];
                    a.ativo = true;
                    a.estado = EstadoAgente::Trabalhando;
                    a.motivo.clear();
                    a.desde = desde_hora;
                    a.desde_em = desde;
                    // Iniciar de novo tira o erro anterior.
                    a.fim = None;
                    t.derivar();
                    efeitos.push(Efeito::Conectar(agente_id));
                }
            }
            Evento::AgenteTerminou { agente_id, tarefa_id, fim, .. } => {
                if let Some((t, i)) = self.agente(tarefa_id, agente_id) {
                    let a = &mut t.agentes[i];
                    let ja_sabia = !a.ativo && a.fim.as_ref() == Some(&fim);
                    a.ativo = false;
                    if fim.erro && !ja_sabia {
                        a.erro_visto = false;
                        efeitos
                            .push(Efeito::Atencao { tarefa: tarefa_id, agente: agente_id, texto: format!("{} em “{}”", fim.texto, t.titulo), erro: true });
                    }
                    t.agentes[i].fim = Some(fim);
                    t.derivar();
                }
            }
            Evento::AgenteEstado { agente_id, tarefa_id, estado, motivo, desde_hora, desde, .. } => {
                if let Some((t, i)) = self.agente(tarefa_id, agente_id) {
                    let a = &mut t.agentes[i];
                    let novo = EstadoAgente::da_chave(&estado);
                    let passou_a_esperar = novo == EstadoAgente::Aguardando && (a.estado != novo || a.motivo != motivo);
                    a.ativo = true;
                    a.estado = novo;
                    a.motivo = motivo;
                    a.desde = desde_hora;
                    a.desde_em = desde;
                    if passou_a_esperar {
                        let acao = if a.motivo == PEDE_APROVACAO { "pede aprovação" } else { "espera sua resposta" };
                        efeitos.push(Efeito::Atencao {
                            tarefa: tarefa_id,
                            agente: agente_id,
                            texto: format!("{} em “{}” {acao}", a.nome_com_papel(), t.titulo),
                            erro: false,
                        });
                    }
                    t.derivar();
                }
            }
        }
        efeitos
    }
}

// Modo demonstração: três projetos e tarefas de exemplo, com agentes de mentira.

/// Papéis dos dez terminais de teste da demonstração.
pub const PAPEIS_DEMO: [&str; 10] = ["líder", "dev", "dev", "revisor", "testador", "dev", "dev", "revisor", "testador", "dev"];

pub const BRANCHES_DEMO: [&str; 5] = ["dev", "main", "feature/pedidos", "feature/clientes", "hotfix/desconto"];

pub fn projetos_demo() -> Vec<Projeto> {
    ["loja-web", "api-pedidos", "estudos-rust"]
        .into_iter()
        .enumerate()
        .map(|(i, nome)| Projeto {
            id: i as i64 + 1,
            nome: nome.into(),
            workspace_id: 1,
            workspace: "Empresa X".into(),
            caminho: String::new(),
            sem_git: false,
            branch_padrao: "main".into(),
        })
        .collect()
}

pub fn gerar_demo(quantidade: usize) -> Vec<Tarefa> {
    let projetos = projetos_demo();
    let nova = |id: i64, projeto: usize, titulo: &str, coluna: Coluna, branch: &str, agentes: Vec<i64>, motivo: Option<&'static str>| Tarefa {
        id,
        projeto_id: projetos[projeto].id,
        projeto: projetos[projeto].nome.clone(),
        titulo: titulo.into(),
        coluna,
        coluna_auto: false,
        branch: branch.into(),
        pasta: String::new(),
        em_copia: true,
        agentes: agentes
            .into_iter()
            .map(|i| AgenteTela {
                id: i,
                ferramenta: format!("agente-{i}"),
                papel: PAPEIS_DEMO[i as usize].into(),
                ativo: true,
                estado: EstadoAgente::Trabalhando,
                motivo: String::new(),
                desde: String::new(),
                desde_em: String::new(),
                fim: None,
                erro_visto: true,
            })
            .collect(),
        motivo: motivo.map(String::from),
        motivo_desde: String::new(),
        motivo_acao: String::new(),
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

    fn evento(json: &str) -> Evento {
        serde_json::from_str(json).expect(json)
    }

    /// Um perfil com o projeto 1 (loja-web) e a tarefa 10 com o agente 7.
    fn modelo() -> Modelo {
        let mut m = Modelo::default();
        let quadro = r#"{"seq":5,
            "projetos":[{"id":1,"workspace":"W","nome":"loja-web","caminho":"/tmp/loja","tipo":"git","branch_padrao":"main"}],
            "tarefas":[{"id":10,"projeto_id":1,"titulo":"Nova tela","coluna":"trabalhando","branch":"main"}],
            "agentes":[{"id":7,"tarefa_id":10,"ferramenta":"claude","papel":"dev","ativo":true,"estado":"trabalhando"}]}"#;
        m.carregar(serde_json::from_str(quadro).unwrap());
        m
    }

    #[test]
    fn criar_mover_e_remover_pelos_eventos() {
        let mut m = modelo();
        m.aplicar(evento(r#"{"tipo":"tarefa.criada","seq":6,"tarefa":{"id":11,"projeto_id":1,"titulo":"Corrigir filtro","coluna":"backlog","branch":"main"},"origem":"voce"}"#));
        assert_eq!(m.tarefas.len(), 2);
        let efeitos = m.aplicar(evento(r#"{"tipo":"tarefa.atualizada","seq":7,"tarefa":{"id":11,"projeto_id":1,"titulo":"Corrigir filtro","coluna":"concluido","branch":"main"},"origem":"voce"}"#));
        assert_eq!(efeitos, vec![Efeito::Concluiu { tarefa: 11, projeto: 1 }]);
        assert_eq!(m.tarefas.iter().find(|t| t.id == 11).unwrap().coluna, Coluna::Concluido);
        m.aplicar(evento(r#"{"tipo":"tarefa.removida","seq":8,"tarefa_id":11,"projeto_id":1}"#));
        assert_eq!(m.tarefas.len(), 1);
    }

    #[test]
    fn a_mesma_mensagem_duas_vezes_nao_muda_nada() {
        let mut m = modelo();
        let criada = r#"{"tipo":"tarefa.criada","seq":6,"tarefa":{"id":11,"projeto_id":1,"titulo":"X","coluna":"backlog","branch":"main"}}"#;
        m.aplicar(evento(criada));
        m.aplicar(evento(criada));
        assert_eq!(m.tarefas.len(), 2);
        let concluida = r#"{"tipo":"tarefa.atualizada","seq":7,"tarefa":{"id":11,"projeto_id":1,"titulo":"X","coluna":"concluido","branch":"main"}}"#;
        assert_eq!(m.aplicar(evento(concluida)).len(), 1);
        // A abelha não comemora duas vezes a mesma conclusão.
        assert!(m.aplicar(evento(concluida)).is_empty());
        let removida = r#"{"tipo":"tarefa.removida","seq":8,"tarefa_id":11}"#;
        m.aplicar(evento(removida));
        m.aplicar(evento(removida));
        assert_eq!(m.tarefas.len(), 1);
    }

    #[test]
    fn tipo_desconhecido_e_ignorado() {
        let mut m = modelo();
        let e = evento(r#"{"tipo":"algo.do.futuro","seq":9,"campo":1}"#);
        assert!(matches!(e, Evento::Desconhecido));
        assert!(m.aplicar(e).is_empty());
        assert_eq!(m.tarefas.len(), 1);
    }

    #[test]
    fn agente_esperando_e_com_erro() {
        let mut m = modelo();
        let efeitos = m.aplicar(evento(
            r#"{"tipo":"agente.estado","seq":6,"agente_id":7,"tarefa_id":10,"estado":"aguardando","motivo":"pede aprovação","desde_hora":"14:32"}"#,
        ));
        assert!(matches!(&efeitos[..], [Efeito::Atencao { erro: false, .. }]));
        let t = &m.tarefas[0];
        assert_eq!(t.agentes[0].texto_estado(), "Parece pedir aprovação · desde 14:32");
        assert_eq!(t.motivo.as_deref(), Some("Claude Code (dev) pede aprovação"));

        let fim = r#"{"tipo":"agente.terminou","seq":7,"agente_id":7,"tarefa_id":10,"fim":{"codigo":1,"erro":true,"motivo":"erro","hora":"14:40","texto":"Claude Code (dev) parou com erro (código 1)"}}"#;
        let efeitos = m.aplicar(evento(fim));
        assert!(matches!(&efeitos[..], [Efeito::Atencao { erro: true, .. }]));
        let t = &m.tarefas[0];
        assert_eq!(t.agentes[0].texto_estado(), "Parou com erro (código 1) às 14:40");
        assert!(t.erro.is_some() && !t.erro_visto && t.motivo.is_none());
        // O mesmo fim de novo não chama atenção outra vez.
        assert!(m.aplicar(evento(fim)).is_empty());
        // Iniciar de novo tira o erro e liga o terminal.
        let efeitos = m.aplicar(evento(r#"{"tipo":"agente.iniciou","seq":8,"agente_id":7,"tarefa_id":10,"desde_hora":"14:45"}"#));
        assert_eq!(efeitos, vec![Efeito::Conectar(7)]);
        assert!(m.tarefas[0].erro.is_none());
    }

    #[test]
    fn erro_de_antes_de_abrir_a_tela_ja_vem_visto() {
        let mut m = Modelo::default();
        let quadro = r#"{"seq":1,"projetos":[{"id":1,"workspace":"W","nome":"p","caminho":"/p","branch_padrao":""}],
            "tarefas":[{"id":10,"projeto_id":1,"titulo":"T","coluna":"trabalhando","branch":""}],
            "agentes":[{"id":7,"tarefa_id":10,"ferramenta":"codex","papel":"dev","ativo":false,
                "ultimo_fim":{"codigo":3,"erro":true,"motivo":"erro","hora":"09:00","texto":"x"}}]}"#;
        m.carregar(serde_json::from_str(quadro).unwrap());
        let t = &m.tarefas[0];
        assert!(t.erro.is_some() && t.erro_visto);
    }

    #[test]
    fn terminal_comum_que_sai_com_codigo_nao_e_erro() {
        let mut a = AgenteTela::da_api(&serde_json::from_str(r#"{"id":1,"tarefa_id":1,"ferramenta":"shell","papel":"dev","ativo":false}"#).unwrap());
        a.fim = Some(api::Fim { codigo: 1, erro: false, motivo: "terminou".into(), hora: "10:00".into(), texto: String::new() });
        assert_eq!(a.visual(), EstadoVisual::Terminou);
        assert_eq!(a.texto_estado(), "Terminal encerrado (código 1) às 10:00");
    }

    #[test]
    fn lousa_do_agente_entra_na_linha_e_a_sua_nao() {
        let do_agente = evento(
            r#"{"tipo":"lousa.mudou","seq":3,"lousa_id":2,"dono":{"tarefa_id":10},"agente_id":7,
                "elementos":[{"id":5,"tipo":"nota","x":1,"y":2,"largura":240,"altura":120,"texto":"oi","autor":"agente","versao":1}],"removidos":[]}"#,
        );
        assert!(do_agente.entra_na_linha());
        let Evento::LousaMudou { dono, elementos, .. } = &do_agente else { panic!("{do_agente:?}") };
        assert_eq!((dono.tarefa_id, elementos[0].tipo, elementos[0].do_agente()), (10, api::TipoElemento::Nota, true));
        let sua = evento(r#"{"tipo":"lousa.mudou","seq":4,"lousa_id":1,"dono":{"workspace_id":1},"elementos":[],"removidos":[5]}"#);
        assert!(!sua.entra_na_linha());
        assert!(!evento(r#"{"tipo":"anexo.adicionado","seq":5,"anexo_id":9,"na_lousa":true}"#).entra_na_linha());
        let mut m = modelo();
        assert!(m.aplicar(sua).is_empty());
    }

    #[test]
    fn demo_tem_tarefas_em_todos_os_projetos() {
        let tarefas = gerar_demo(50);
        assert_eq!(tarefas.len(), 50);
        for p in projetos_demo() {
            assert!(tarefas.iter().any(|t| t.projeto_id == p.id && t.projeto == p.nome));
        }
    }

    #[test]
    fn pedido_e_navegador_pelos_eventos() {
        let mut m = modelo();
        let pedido = |estado: &str| {
            format!(
                r#"{{"tipo":"pedido.{estado}","seq":9,"pedido":{{"id":3,"tarefa_id":10,"agente_id":7,"tipo":"daily","periodo":"2026-10-02","texto":"Traga o total de testes","estado":"{estado}"}}}}"#
            )
        };
        let criado = pedido("criado").replace(r#""estado":"criado""#, r#""estado":"fila""#);
        assert!(m.aplicar(evento(&criado)).is_empty());
        assert_eq!(m.pedido_da_tarefa(10).map(|p| p.estado.as_str()), Some("fila"));
        m.aplicar(evento(&pedido("entregue")));
        assert_eq!(m.pedidos.len(), 1);
        // Respondido: a tela avisa uma vez só, mesmo com a mensagem repetida.
        assert!(matches!(m.aplicar(evento(&pedido("respondido")))[..], [Efeito::PedidoRespondido { tarefa: 10, .. }]));
        assert!(m.aplicar(evento(&pedido("respondido"))).is_empty());
        assert_eq!(m.pedido_da_tarefa(10).map(|p| p.estado.as_str()), Some("respondido"));

        m.aplicar(evento(r#"{"tipo":"navegador.aberto","seq":10,"tarefa_id":10,"descricao":"localhost:5173/pedidos"}"#));
        assert!(m.navegadores.contains(&10));
        m.aplicar(evento(r#"{"tipo":"navegador.fechado","seq":11,"tarefa_id":10}"#));
        assert!(!m.navegadores.contains(&10));
        let nota = evento(r#"{"tipo":"nota.atualizada","seq":12,"tarefa_id":10,"agente_id":7,"modo":"complementar"}"#);
        assert!(matches!(nota, Evento::NotaAtualizada { agente_id: 7, .. }));
        m.aplicar(evento(r#"{"tipo":"tarefa.removida","seq":13,"tarefa_id":10}"#));
        assert!(m.pedidos.is_empty());
    }
}
