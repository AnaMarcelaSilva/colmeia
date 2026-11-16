//! A abelha fixa da Colmeia: um programa à parte, pequeno e sem moldura,
//! sempre por cima e em todas as áreas de trabalho (no X11), que continua
//! com a janela da Colmeia fechada. Fala com o núcleo pelo mesmo canal e
//! token da janela e só escuta os eventos do perfil: parada, mostra o ponto
//! da cor do que mais pede você; um clique abre o resumo, que fica até ser
//! fechado; os avisos saem dela em balões. Só redesenha quando algo muda.
//!
//! Com os comandos falados ligados no perfil, o clique (ou Ctrl+Alt+Espaço,
//! de qualquer programa, no X11) abre o microfone para um comando e o resumo
//! passa ao botão direito; com o "ouvir o nome", ela fica ouvindo o nome dela,
//! com o ponto vermelho do microfone sempre à vista. Com os comandos ou a
//! pergunta livre com IA, o resumo tem uma caixa para escrever à abelha.
//!
//! Uso: colmeia-abelha --perfil N (a janela abre com a opção "Fixar a
//! abelha na área de trabalho").

mod atalho;
mod conversa;
mod fixar;
mod ligacao;
mod posicao;
mod resumo;
mod tela;

use std::collections::HashSet;
#[cfg(any(unix, windows))]
use std::fs::File;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use canal::voz::{Conversa, Fase, Saida};

use eframe::egui::{self, Pos2, Rect, Sense, ViewportCommand, pos2, vec2};
use mascote::{Estado, Modelo, Tempo};

use ligacao::Mensagem;
use resumo::{Aviso, Efeito, OpcoesVoz, Resumo};

/// O quadrado da abelha na tela (com folga para o ponto e o pulo).
const ABELHA: f32 = 76.0;
/// Entre a abelha e o cartão.
const VAO: f32 = 8.0;
/// O arraste só começa depois disto (em pontos): o clique, mesmo o duplo, não move.
const LIMIAR_ARRASTE: f32 = 6.0;
/// Animação depois de uma troca de estado ou do mouse chegar; depois, parada.
const ANIMACAO: f64 = 2.5;
const COMEMORACAO: f64 = 3.0;
const BALAO: f64 = 8.0;
const QUADROS_ANIMACAO: f64 = 30.0;
/// O fio do balão não precisa de mais que isso para andar liso.
const QUADROS_BALAO: f64 = 10.0;

struct Balao {
    aviso: Aviso,
    resta: f64,
}

/// O que aconteceu enquanto você esteve fora, para o cartão da volta.
struct Volta {
    fora: Duration,
    avisos: Vec<Aviso>,
}

/// Sem mexer no computador por esse tempo, conta como pausa: ao voltar, a
/// abelha conta o que aconteceu. COLMEIA_PAUSA (em segundos) troca, para testar.
const PAUSA: Duration = Duration::from_secs(15 * 60);
/// De quanto em quanto tempo a abelha olha se você está aí.
const PASSO_PAUSA: Duration = Duration::from_secs(10);

/// Vigia a pausa numa thread: quando você fica sem mexer no computador
/// por `limiar` e volta, manda quanto tempo ficou fora. Onde não dá para
/// saber (Wayland, Windows), a thread sai.
fn vigiar_pausa(limiar: Duration, ctx: egui::Context, envio: Sender<Mensagem>) {
    let _ = std::thread::Builder::new().name("pausa".into()).spawn(move || {
        let mut fora = Duration::ZERO;
        loop {
            std::thread::sleep(PASSO_PAUSA);
            let Some(ocioso) = fixar::ocioso() else { return };
            if ocioso >= limiar {
                fora = ocioso;
            } else if fora >= limiar {
                if envio.send(Mensagem::Voltou(fora)).is_err() {
                    return;
                }
                ctx.request_repaint();
                fora = Duration::ZERO;
            }
        }
    });
}

/// Quanto a resposta falada fica no balão (com pergunta, até responder).
const RESPOSTA: f64 = 10.0;
const PERGUNTA: f64 = 60.0;

/// A resposta de um comando falado (ou escrito), no balão.
struct Resposta {
    texto: String,
    ouvido: String,
    escrito: bool,
    erro: bool,
    abrir: Option<i64>,
    /// A resposta é a própria lista (o rodapé ensina o que dizer).
    de_lista: bool,
    ate: f64,
}

/// A conversa aberta com um agente: as últimas linhas do terminal dele,
/// pedidas ao núcleo a cada segundo, e o que você está escrevendo.
struct Papo {
    tarefa: i64,
    /// O agente mais novo da tarefa, rodando ou não; sem nenhum, "Rodar" cria.
    agente: Option<i64>,
    linhas: Vec<String>,
    /// As linhas interpretadas em blocos de conversa.
    blocos: Vec<conversa::Bloco>,
    /// Os grupos de ferramentas abertos na tela.
    abertos: HashSet<usize>,
    rodando: bool,
    /// Pedimos ao núcleo para ligar (ou criar) o agente e esperamos a resposta.
    iniciando: bool,
    estado: String,
    motivo: String,
    texto: String,
    enviando: bool,
    pedindo: bool,
    ultimo_pedido: f64,
    erro: Option<String>,
}

/// A tarefa nova sendo escrita.
struct Nova {
    titulo: String,
    projeto: usize,
    com_agente: bool,
    enviando: bool,
    erro: Option<String>,
}

/// De quanto em quanto tempo a conversa pede o texto ao núcleo.
const PASSO_PAPO: f64 = 1.0;

struct Fixa {
    perfil: i64,
    modelo: Modelo,
    nome: String,
    resumo: Resumo,
    recebe: Receiver<Mensagem>,
    ligada: bool,
    estado: Estado,
    mudou_em: f64,
    animar_ate: f64,
    comemorar_ate: f64,
    em_cima: bool,
    /// Arrastando: onde o botão apertou, dentro da abelha. O arraste é feito
    /// aqui (a janela é uma "dock" no X11, que o gerenciador não move) e só
    /// começa depois de alguns pixels: um clique duplo não move nada.
    arraste: Option<egui::Vec2>,
    /// Janelas da Colmeia à vista: com alguma, a abelha se esconde.
    janelas: usize,
    escondida: bool,
    aberto: bool,
    balao: Option<Balao>,
    /// O cartão de quando você volta de uma pausa.
    volta: Option<Volta>,
    /// O canto de cima à esquerda da abelha, na tela.
    posicao: Option<Pos2>,
    /// A janela como foi pedida por último (na tela).
    janela: Option<Rect>,
    ultimo_quadro: f64,
    quadros: u32,
    /// Sem compositor, a janela é opaca, no fundo escuro dos cartões.
    transparente: bool,
    /// O tamanho da tela quando o egui não sabe o do monitor (lido uma vez).
    tela: Option<egui::Vec2>,
    /// Os comandos falados do perfil e a conversa (com eles ligados).
    voz: OpcoesVoz,
    conversa: Option<Conversa>,
    resposta: Option<Resposta>,
    /// A frase sendo escrita no resumo.
    rascunho: String,
    atalho: Option<atalho::Atalho>,
    /// A conversa com um agente e a tarefa nova, quando abertas (no lugar do resumo).
    papo: Option<Papo>,
    nova: Option<Nova>,
    /// O tamanho da conversa (ela cresce pelo canto livre) e se está sendo puxada.
    tamanho_papo: egui::Vec2,
    puxando: bool,
    /// A escala da letra da conversa (uma de tela::LETRAS).
    letra_papo: f32,
    /// O canto livre do cartão na última vez: (à esquerda, em cima).
    canto_livre: (bool, bool),
    envio: Sender<Mensagem>,
    _trava: Trava,
}

/// Uma abelha só por perfil: a segunda vê a trava e sai.
/// Segura o arquivo aberto (e a trava) enquanto a abelha vive.
struct Trava {
    #[cfg(any(unix, windows))]
    _arquivo: File,
}

#[cfg(unix)]
fn travar(perfil: i64) -> Option<Trava> {
    use std::os::unix::fs::OpenOptionsExt;
    use std::os::unix::io::AsRawFd;
    let caminho = canal::diretorio().join(format!("abelha-{perfil}.trava"));
    let arquivo = std::fs::OpenOptions::new().write(true).create(true).truncate(false).mode(0o600).open(caminho).ok()?;
    // SAFETY: flock num descritor aberto e nosso; não mexe em memória.
    let ok = unsafe { libc::flock(arquivo.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0;
    ok.then_some(Trava { _arquivo: arquivo })
}

/// No Windows, o arquivo aberto sem compartilhar já é a trava.
#[cfg(windows)]
fn travar(perfil: i64) -> Option<Trava> {
    use std::os::windows::fs::OpenOptionsExt;
    let caminho = canal::diretorio().join(format!("abelha-{perfil}.trava"));
    let arquivo = std::fs::OpenOptions::new().write(true).create(true).truncate(false).share_mode(0).open(caminho).ok()?;
    Some(Trava { _arquivo: arquivo })
}

#[cfg(not(any(unix, windows)))]
fn travar(_: i64) -> Option<Trava> {
    Some(Trava {})
}

impl Fixa {
    fn vestir(&mut self, ctx: &egui::Context, modelo: &str, nome: &str) {
        self.modelo = Modelo::do_codigo(modelo);
        self.nome = if nome.trim().is_empty() { "Colmeia".into() } else { nome.to_string() };
        ctx.send_viewport_cmd(ViewportCommand::Title(self.nome.clone()));
        self.animar_ate = self.ultimo_quadro + ANIMACAO;
    }

    fn receber(&mut self, ctx: &egui::Context, agora: f64) {
        while let Ok(m) = self.recebe.try_recv() {
            match m {
                Mensagem::Perfil(Some(p)) => {
                    self.vestir(ctx, &p.mascote_modelo, &p.mascote_nome);
                    self.ajustar_voz(ctx, p.voz, agora);
                }
                Mensagem::Atalho => self.falar(),
                // O perfil foi apagado: não há mais o que mostrar.
                Mensagem::Perfil(None) => ctx.send_viewport_cmd(ViewportCommand::Close),
                Mensagem::Quadro(q) => {
                    self.resumo.carregar(q);
                    self.ligada = true;
                }
                Mensagem::Fora => {
                    self.ligada = false;
                    // Sem núcleo, nenhuma janela conta: a abelha aparece.
                    self.janelas = 0;
                }
                Mensagem::Janelas(n) => self.janelas = n,
                Mensagem::Texto { agente, resultado } => {
                    if let Some(p) = self.papo.as_mut().filter(|p| p.agente == Some(agente)) {
                        p.pedindo = false;
                        match resultado {
                            Ok(t) => {
                                if p.linhas != t.linhas {
                                    p.blocos = conversa::interpretar(&t.linhas);
                                }
                                p.linhas = t.linhas;
                                p.rodando = t.rodando;
                                p.estado = t.estado;
                                p.motivo = t.motivo;
                            }
                            Err(e) => p.erro = Some(e),
                        }
                    }
                }
                Mensagem::Entrada(r) => {
                    if let Some(p) = self.papo.as_mut() {
                        p.enviando = false;
                        if let Err(e) = r {
                            p.erro = Some(e);
                        }
                    }
                }
                Mensagem::Criada(r) => {
                    if let Some(n) = self.nova.as_mut() {
                        n.enviando = false;
                    }
                    match r {
                        Ok((tarefa, Some(agente))) => {
                            self.nova = None;
                            self.conversar(agora, tarefa, Some(agente));
                        }
                        Ok((_, None)) => {
                            self.nova = None;
                            self.aberto = true;
                        }
                        Err(e) => {
                            if let Some(n) = self.nova.as_mut() {
                                n.erro = Some(e);
                            }
                        }
                    }
                }
                Mensagem::Iniciado { tarefa, resultado } => {
                    if let Some(p) = self.papo.as_mut().filter(|p| p.tarefa == tarefa) {
                        p.iniciando = false;
                        match resultado {
                            Ok(agente) => {
                                p.agente = Some(agente);
                                p.rodando = true;
                                p.linhas.clear();
                                p.blocos.clear();
                                p.erro = None;
                                p.ultimo_pedido = agora - PASSO_PAPO;
                            }
                            Err(e) => p.erro = Some(e),
                        }
                    }
                }
                Mensagem::Voltou(fora) => {
                    // O que aconteceu desde que você saiu e ainda vale. Com a
                    // Colmeia à vista você já está olhando o quadro.
                    let avisos = self.resumo.desde(Instant::now().checked_sub(fora).unwrap_or_else(Instant::now));
                    if !avisos.is_empty() && self.janelas == 0 {
                        self.aberto = false;
                        self.balao = None;
                        self.volta = Some(Volta { fora, avisos });
                    }
                }
                Mensagem::Evento(e) => {
                    for efeito in self.resumo.aplicar(e) {
                        match efeito {
                            // Com o cartão da volta na tela, o aviso novo entra nele.
                            Efeito::Avisar(aviso) if self.volta.is_some() => {
                                let v = self.volta.as_mut().expect("cartão da volta");
                                v.avisos.retain(|a| !(a.tarefa == aviso.tarefa && a.tipo == aviso.tipo));
                                v.avisos.insert(0, aviso);
                            }
                            // Com o resumo aberto, o que mudou já está nele.
                            Efeito::Avisar(aviso) if !self.aberto => self.balao = Some(Balao { aviso, resta: BALAO }),
                            Efeito::Avisar(_) | Efeito::Recarregar => {}
                            Efeito::Comemorar => self.comemorar_ate = agora + COMEMORACAO,
                            Efeito::Vestir { modelo, nome } => self.vestir(ctx, &modelo, &nome),
                            Efeito::Sair => ctx.send_viewport_cmd(ViewportCommand::Close),
                            Efeito::Voz(v) => self.ajustar_voz(ctx, v, agora),
                        }
                    }
                }
            }
        }
    }

    /// Abre a conversa com o agente de uma tarefa, no lugar do resumo. Sem
    /// agente indicado, com o mais novo da tarefa, parado ou não.
    fn conversar(&mut self, agora: f64, tarefa: i64, agente: Option<i64>) {
        self.aberto = false;
        self.balao = None;
        self.volta = None;
        self.nova = None;
        self.resumo.visto(tarefa);
        let agente = agente.or_else(|| self.resumo.ultimo_agente(tarefa).map(|a| a.id));
        self.papo = Some(Papo {
            tarefa,
            agente,
            linhas: Vec::new(),
            blocos: Vec::new(),
            abertos: HashSet::new(),
            rodando: agente.is_some_and(|a| self.resumo.ativo(a)),
            iniciando: false,
            estado: String::new(),
            motivo: String::new(),
            texto: String::new(),
            enviando: false,
            pedindo: false,
            ultimo_pedido: agora - PASSO_PAPO,
            erro: None,
        });
    }

    /// Pede ao núcleo as linhas do agente da conversa, de tempos em tempos.
    fn acompanhar_papo(&mut self, ctx: &egui::Context, agora: f64) {
        let Some(p) = self.papo.as_mut() else { return };
        // Sem agente não há texto a pedir; ligado pela Colmeia, o retrato avisa.
        let Some(agente) = p.agente else { return };
        if !p.rodando && self.resumo.ativo(agente) {
            p.rodando = true;
        }
        if !p.pedindo && agora - p.ultimo_pedido >= PASSO_PAPO {
            p.pedindo = true;
            p.ultimo_pedido = agora;
            let (envio, ctx2) = (self.envio.clone(), ctx.clone());
            std::thread::spawn(move || {
                let _ = envio.send(Mensagem::Texto { agente, resultado: ligacao::texto_do_agente(agente, tela::LINHAS_PAPO) });
                ctx2.request_repaint();
            });
        }
        ctx.request_repaint_after(Duration::from_secs_f64(PASSO_PAPO));
    }

    /// O cartão que aparece com a abelha (a voz, o resumo ou o balão) e a altura dele.
    fn cartao(&self, ctx: &egui::Context) -> Option<(f32, f32)> {
        let l = tela::LARGURA_CARTAO;
        if self.com_voz() {
            let (texto, rodape) = self.texto_da_voz();
            Some((l, tela::altura_balao_voz(&texto, &rodape)))
        } else if self.papo.is_some() {
            Some((self.tamanho_papo.x, self.tamanho_papo.y))
        } else if self.nova.is_some() {
            Some((l, tela::ALTURA_NOVA))
        } else if self.aberto {
            Some((l, tela::altura_resumo(self.resumo.destaques().len(), self.conversa.is_some())))
        } else if let Some(v) = &self.volta {
            Some((l, tela::altura_volta(v.avisos.len())))
        } else {
            self.balao.as_ref().map(|b| (l, tela::altura_balao_medida(ctx, &b.aviso)))
        }
    }

    /// O balão da voz está na tela: ouvindo, entendendo ou a resposta.
    fn com_voz(&self) -> bool {
        self.resposta.is_some() || self.conversa.as_ref().is_some_and(|c| !matches!(c.fase(), Fase::Parada))
    }

    /// Liga ou desliga os comandos falados, o atalho e o ouvir o nome (e a
    /// conversa escrita, que basta para a pergunta livre).
    fn ajustar_voz(&mut self, ctx: &egui::Context, mut voz: OpcoesVoz, agora: f64) {
        // Guardados por enquanto (canal::voz::COMANDOS_FALADOS): sem microfone, sem falar e sem ouvir o nome.
        if !canal::voz::COMANDOS_FALADOS {
            voz.voz_comandos = false;
            voz.voz_abelha = false;
            voz.voz_nome = false;
        }
        if !canal::voz::PERGUNTA_LIVRE {
            voz.voz_ia = false;
        }
        self.voz = voz.clone();
        if !voz.voz_comandos && !voz.voz_ia {
            if let Some(c) = self.conversa.as_mut() {
                c.fechar();
            }
            self.conversa = None;
            self.resposta = None;
            self.atalho = None;
            self.rascunho.clear();
            return;
        }
        let perfil = self.perfil;
        let acordar = ctx.clone();
        let c = self.conversa.get_or_insert_with(|| Conversa::nova(perfil, Arc::new(move || acordar.request_repaint())));
        c.falar = voz.voz_abelha;
        c.efeito = voz.voz_efeito;
        // Só a pergunta livre: sem microfone, sem atalho e sem ouvir o nome.
        if !voz.voz_comandos {
            c.cancelar();
            if c.vigiando() {
                c.desvigiar();
            }
            self.atalho = None;
            return;
        }
        if self.atalho.is_none() {
            let (envio, ctx) = (self.envio.clone(), ctx.clone());
            match atalho::ouvir(move || {
                let _ = envio.send(Mensagem::Atalho);
                ctx.request_repaint();
            }) {
                Ok(a) => self.atalho = Some(a),
                Err(e) => eprintln!("abelha fixa: {e}"),
            }
        }
        let Some(c) = self.conversa.as_mut() else { return };
        if !voz.voz_nome {
            if c.vigiando() {
                c.desvigiar();
            }
            return;
        }
        // Com o nome ligado, todo aviso da voz pede de novo (é raro): as
        // gravações do nome podem ter mudado, e Ajustes avisa por um evento do perfil.
        let erro = match canal::voz::nome_ouvido(perfil).map(|n| (n.referencias(), n.nome)) {
            Ok((Some(r), _)) => c.vigiar(r, voz.voz_sensibilidade).err(),
            Ok((None, nome)) => Some(format!("Grave o nome {nome} em Ajustes › Comandos falados para a abelha ouvir.")),
            Err(e) => Some(e),
        };
        if let Some(texto) = erro {
            self.resposta = Some(Resposta { texto, ouvido: String::new(), escrito: false, erro: true, abrir: None, de_lista: false, ate: agora + RESPOSTA });
        }
    }

    /// O clique (ou o atalho): chama a abelha, que fala a lista e abre o
    /// microfone; clicado ouvindo, para e transcreve o que ouviu (o clique
    /// manda, sem esperar a pausa).
    fn falar(&mut self) {
        let Some(c) = self.conversa.as_mut().filter(|_| self.voz.voz_comandos) else { return };
        self.aberto = false;
        self.balao = None;
        if matches!(c.fase(), Fase::Abrindo | Fase::Ouvindo) {
            c.parar();
            return;
        }
        if c.ouvir_agora() {
            return;
        }
        c.encerrar();
        c.chamar();
        self.resposta = None;
    }

    /// O texto do balão da voz e o rodapé: com a lista aberta, ela fica no
    /// balão e embaixo vai o que acontece agora (ou o que dá para dizer).
    fn texto_da_voz(&self) -> (String, String) {
        let Some(c) = self.conversa.as_ref() else { return (String::new(), String::new()) };
        let andamento = match c.fase() {
            Fase::Abrindo | Fase::Ouvindo if c.lista().is_some() => Some("Ouvindo…"),
            Fase::Abrindo | Fase::Ouvindo => Some("Ouvindo… diga o comando."),
            Fase::Transcrevendo => Some("Entendendo…"),
            Fase::Pensando => Some("Um instante…"),
            Fase::Parada => None,
        };
        let r = self.resposta.as_ref();
        let Some(l) = c.lista() else {
            return (andamento.map_or_else(|| r.map_or(String::new(), |r| r.texto.clone()), String::from), String::new());
        };
        let rodape = match (andamento, r) {
            (Some(a), _) => format!("{a} {}", l.dica),
            (None, Some(r)) if !r.de_lista && !r.ouvido.is_empty() => format!("{}\nVocê disse: \u{201c}{}\u{201d}", r.texto, r.ouvido),
            (None, Some(r)) if !r.de_lista => r.texto.clone(),
            _ => l.dica.clone(),
        };
        (l.texto("Nada espera você agora."), rodape)
    }

    /// O que chegou da voz e do núcleo.
    fn receber_voz(&mut self, agora: f64) {
        let Some(c) = self.conversa.as_mut() else { return };
        for s in c.receber() {
            match s {
                Saida::Ouvido(ouvido) => {
                    self.resposta =
                        Some(Resposta { texto: String::new(), ouvido, escrito: false, erro: false, abrir: None, de_lista: false, ate: agora + PERGUNTA });
                }
                Saida::Resposta(r) => {
                    let (ouvido, escrito) = self.resposta.take().map(|r| (r.ouvido, r.escrito)).unwrap_or_default();
                    let abrir = (r.abrir > 0).then_some(r.abrir);
                    // "Abrir a tarefa": a abelha leva até ela (como o Abrir dos
                    // balões); "responder", já com o ditado ligado.
                    if let Some(t) = abrir {
                        let agente = (r.agente > 0).then_some(r.agente);
                        ligacao::abrir_colmeia(self.perfil, t, agente, r.ditar && agente.is_some());
                    }
                    // Uma resposta longa (a pergunta livre) fica o tempo de ler.
                    let espera = !r.confirmar.is_empty() || r.ouvir;
                    let ate = agora + if espera { PERGUNTA } else { RESPOSTA.max(r.texto.chars().count() as f64 / 15.0) };
                    let de_lista = r.lista.is_some();
                    self.resposta = Some(Resposta { texto: r.texto, ouvido, escrito, erro: false, abrir, de_lista, ate });
                }
                Saida::Erro(texto) => {
                    self.resposta =
                        Some(Resposta { texto, ouvido: String::new(), escrito: false, erro: true, abrir: None, de_lista: false, ate: agora + RESPOSTA })
                }
                Saida::Chamado => {
                    self.aberto = false;
                    self.balao = None;
                    self.resposta = None;
                }
                Saida::Vigiando(_) | Saida::Falando(_) | Saida::Amostra(_) | Saida::Aprendido | Saida::Nota { .. } => {}
            }
        }
        if self.resposta.as_ref().is_some_and(|r| agora >= r.ate) {
            self.resposta = None;
            if let Some(c) = self.conversa.as_mut() {
                c.pergunta = None;
            }
        }
    }

    /// O balão da voz: a lista, ouvindo, entendendo ou a resposta.
    fn balao_voz(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let (texto, rodape) = self.texto_da_voz();
        let Some(c) = self.conversa.as_mut() else { return };
        let parada = matches!(c.fase(), Fase::Parada);
        let r = self.resposta.as_ref();
        let com_lista = c.lista().is_some();
        let fala = tela::Fala {
            texto: &texto,
            rodape: &rodape,
            ouvido: if com_lista { "" } else { r.map_or("", |r| r.ouvido.as_str()) },
            escrito: r.is_some_and(|r| r.escrito),
            erro: parada && !com_lista && r.is_some_and(|r| r.erro),
            pergunta: parada && c.pergunta.is_some(),
            abrir: if parada { r.and_then(|r| r.abrir) } else { None },
            microfone: c.microfone_aberto(),
        };
        match tela::balao_voz(ui, rect, &self.nome, &fala) {
            Some(tela::PedidoVoz::Sim) => {
                c.responder(true);
                self.resposta = None;
            }
            Some(tela::PedidoVoz::Nao) => {
                c.responder(false);
                self.resposta = None;
            }
            Some(tela::PedidoVoz::Abrir(t)) => {
                ligacao::abrir_colmeia(self.perfil, t, None, false);
                self.resposta = None;
            }
            Some(tela::PedidoVoz::Fechar) => {
                c.encerrar();
                self.resposta = None;
            }
            None => {}
        }
    }

    /// A janela na tela para a abelha em `p`, com o cartão (se houver) para
    /// o lado de dentro da tela, e onde a abelha e o cartão ficam nela.
    fn arranjo(p: Pos2, cartao: Option<(f32, f32)>, monitor: egui::Vec2) -> (Rect, Rect, Option<Rect>) {
        let abelha = Rect::from_min_size(p, vec2(ABELHA, ABELHA));
        let Some((largura, altura)) = cartao else { return (abelha, Rect::from_min_size(Pos2::ZERO, abelha.size()), None) };
        let (direita, acima) = Self::lado(p, monitor);
        let x = if direita { abelha.right() - largura } else { abelha.left() };
        let y = if acima { abelha.top() - VAO - altura } else { abelha.bottom() + VAO };
        let cartao = Rect::from_min_size(pos2(x, y), vec2(largura, altura));
        let janela = abelha.union(cartao);
        let local = |r: Rect| r.translate(-janela.min.to_vec2());
        (janela, local(abelha), Some(local(cartao)))
    }

    /// De que lado da abelha o cartão abre: (para a esquerda, para cima).
    fn lado(p: Pos2, monitor: egui::Vec2) -> (bool, bool) {
        let abelha = Rect::from_min_size(p, vec2(ABELHA, ABELHA));
        (abelha.center().x > monitor.x / 2.0, abelha.center().y > monitor.y / 2.0)
    }

    /// Puxando a alça da conversa: o tamanho segue o ponteiro na tela, com o
    /// canto preso à abelha parado. Solto o botão, o tamanho fica guardado.
    fn puxar(&mut self, ctx: &egui::Context, cartao: Rect, janela: Rect, monitor: egui::Vec2, entrada: (Option<Pos2>, bool, Option<Rect>)) {
        let (ponteiro, apertado, fora) = entrada;
        let ppp = ctx.pixels_per_point();
        let tela = fixar::ponteiro().map(|(x, y, b)| (pos2(x, y) / ppp, b)).or_else(|| Some((fora?.min + ponteiro?.to_vec2(), apertado)));
        let Some((global, segue)) = tela else {
            self.puxando = false;
            return;
        };
        if !segue {
            self.puxando = false;
            posicao::gravar_tamanho(self.perfil, self.tamanho_papo);
            return;
        }
        let cartao = cartao.translate(janela.min.to_vec2());
        let (esquerda, cima) = self.canto_livre;
        let largura = if esquerda { cartao.right() - global.x } else { global.x - cartao.left() } + 9.0;
        let altura = if cima { cartao.bottom() - global.y } else { global.y - cartao.top() } + 9.0;
        let maximo = vec2((monitor.x - 24.0).max(tela::PAPO_MINIMO.x), (monitor.y - 24.0).max(tela::PAPO_MINIMO.y));
        self.tamanho_papo = vec2(largura, altura).max(tela::PAPO_MINIMO).min(maximo).round();
        ctx.request_repaint_after(Duration::from_millis(16));
    }

    fn tempo(&self, agora: f64, animando: bool) -> Tempo {
        // Parada: um instante fixo depois da animação (no bugado, sem rajada forte).
        let mut t = if animando { agora } else { self.mudou_em + ANIMACAO };
        if !animando && self.estado == Estado::Bugado && t % 0.9 < 0.3 {
            t += 0.35 - t % 0.9;
        }
        Tempo { total: t as f32, no_estado: (t - self.mudou_em) as f32 }
    }

    /// A conversa com o agente: desenha e atende o que você pediu nela.
    fn mostrar_papo(&mut self, ui: &mut egui::Ui, rect: Rect, agora: f64) {
        let Some(p) = self.papo.as_mut() else { return };
        let outras = self.resumo.contagem().esperam.saturating_sub(usize::from(self.resumo.pede_voce(p.tarefa)));
        let (titulo, quem_txt) = self.resumo.sobre(p.tarefa, p.agente);
        let tarefas = self.resumo.tarefas_lista();
        let mut vista = tela::Papo {
            titulo: &titulo,
            tarefa: p.tarefa,
            tarefas: &tarefas,
            quem: &quem_txt,
            estado: &p.estado,
            motivo: &p.motivo,
            rodando: p.rodando,
            com_agente: p.agente.is_some(),
            iniciando: p.iniciando,
            blocos: &p.blocos,
            abertos: &mut p.abertos,
            texto: &mut p.texto,
            enviando: p.enviando,
            erro: p.erro.as_deref(),
            outras,
            canto: self.canto_livre,
            letra: self.letra_papo,
        };
        let pedido = tela::papo(ui, rect, &self.nome, &mut vista);
        let (tarefa, agente) = (p.tarefa, p.agente);
        match pedido {
            Some(tela::PedidoPapo::Fechar) => {
                self.papo = None;
                self.aberto = true;
            }
            Some(tela::PedidoPapo::Enviar) => {
                let texto = p.texto.trim().to_string();
                p.texto.clear();
                p.enviando = true;
                p.erro = None;
                if let Some(agente) = agente {
                    self.escrever_no_agente(ui.ctx(), agente, texto, true);
                }
            }
            Some(tela::PedidoPapo::Opcao(n)) => {
                p.erro = None;
                if let Some(agente) = agente {
                    self.escrever_no_agente(ui.ctx(), agente, n.to_string(), false);
                }
            }
            Some(tela::PedidoPapo::Iniciar) => {
                p.iniciando = true;
                p.erro = None;
                let (envio, ctx) = (self.envio.clone(), ui.ctx().clone());
                std::thread::spawn(move || {
                    let resultado = match agente {
                        Some(a) => ligacao::iniciar_agente(a),
                        None => ligacao::criar_agente(tarefa),
                    };
                    let _ = envio.send(Mensagem::Iniciado { tarefa, resultado });
                    ctx.request_repaint();
                });
            }
            Some(tela::PedidoPapo::Proximo) => {
                // A tarefa seguinte na ordem do resumo, pulando esta; no fim, volta ao começo.
                let lista = self.resumo.destaques();
                let depois = lista.iter().position(|d| d.tarefa == tarefa).map_or(0, |i| i + 1);
                if let Some(d) = lista.iter().cycle().skip(depois).take(lista.len()).find(|d| d.tarefa != tarefa) {
                    let (t, a) = (d.tarefa, d.agente);
                    self.conversar(agora, t, a);
                }
            }
            Some(tela::PedidoPapo::Trocar(t, a)) => {
                if t != tarefa {
                    self.conversar(agora, t, a);
                }
            }
            Some(tela::PedidoPapo::Puxar) => self.puxando = true,
            Some(tela::PedidoPapo::Letra(escala)) => {
                self.letra_papo = escala;
                posicao::gravar_letra(self.perfil, escala);
            }
            Some(tela::PedidoPapo::Tema(i)) => {
                tela::escolher_tema(i);
                aplicar_visual(ui.ctx());
                posicao::gravar_tema(self.perfil, tela::TEMAS[i].nome);
            }
            Some(tela::PedidoPapo::AbrirColmeia) => ligacao::abrir_colmeia(self.perfil, tarefa, agente, false),
            Some(tela::PedidoPapo::NovaTarefa) => {
                self.papo = None;
                self.nova = Some(Nova { titulo: String::new(), projeto: 0, com_agente: true, enviando: false, erro: None });
            }
            Some(tela::PedidoPapo::Desafixar) => self.atender(&ui.ctx().clone(), tela::Pedido::Desafixar),
            None => {}
        }
    }

    fn escrever_no_agente(&self, ctx: &egui::Context, agente: i64, texto: String, enter: bool) {
        let (envio, ctx) = (self.envio.clone(), ctx.clone());
        std::thread::spawn(move || {
            let _ = envio.send(Mensagem::Entrada(ligacao::entrada_do_agente(agente, &texto, enter)));
            ctx.request_repaint();
        });
    }

    /// A tarefa nova: desenha e atende.
    fn mostrar_nova(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let projetos = self.resumo.projetos_lista();
        let Some(n) = self.nova.as_mut() else { return };
        if n.projeto >= projetos.len() {
            n.projeto = 0;
        }
        let nome_projeto = projetos.get(n.projeto).map(|(_, nome, _)| nome.as_str()).unwrap_or("");
        let mut vista = tela::NovaVista {
            titulo: &mut n.titulo,
            projeto: nome_projeto,
            varios_projetos: projetos.len() > 1,
            com_agente: n.com_agente,
            enviando: n.enviando,
            erro: n.erro.as_deref(),
        };
        match tela::nova_tarefa(ui, rect, &self.nome, &mut vista) {
            Some(tela::PedidoNova::Cancelar) => {
                self.nova = None;
                self.aberto = true;
            }
            Some(tela::PedidoNova::TrocarProjeto) => {
                if !projetos.is_empty() {
                    n.projeto = (n.projeto + 1) % projetos.len();
                }
            }
            Some(tela::PedidoNova::AlternarAgente) => n.com_agente = !n.com_agente,
            Some(tela::PedidoNova::Criar) => {
                let Some((projeto, _, branch)) = projetos.get(n.projeto) else { return };
                n.enviando = true;
                n.erro = None;
                let (projeto, branch, titulo, com_agente) = (*projeto, branch.clone(), n.titulo.trim().to_string(), n.com_agente);
                let (envio, ctx) = (self.envio.clone(), ui.ctx().clone());
                std::thread::spawn(move || {
                    let _ = envio.send(Mensagem::Criada(ligacao::criar_tarefa(projeto, &titulo, &branch, com_agente)));
                    ctx.request_repaint();
                });
            }
            None => {}
        }
    }

    fn atender(&mut self, ctx: &egui::Context, pedido: tela::Pedido) {
        match pedido {
            tela::Pedido::Fechar => {
                self.aberto = false;
                self.balao = None;
                self.volta = None;
            }
            tela::Pedido::Conversar { tarefa, agente } => self.conversar(self.ultimo_quadro, tarefa, agente),
            tela::Pedido::NovaTarefa => {
                self.aberto = false;
                self.balao = None;
                self.nova = Some(Nova { titulo: String::new(), projeto: 0, com_agente: true, enviando: false, erro: None });
            }
            tela::Pedido::Escrever => {
                let frase = self.rascunho.trim().to_string();
                if let Some(c) = self.conversa.as_mut()
                    && c.escrever(&frase)
                {
                    self.aberto = false;
                    self.rascunho.clear();
                    self.resposta = Some(Resposta {
                        texto: String::new(),
                        ouvido: frase,
                        escrito: true,
                        erro: false,
                        abrir: None,
                        de_lista: false,
                        ate: self.ultimo_quadro + PERGUNTA,
                    });
                }
            }
            tela::Pedido::Abrir { tarefa, agente } => {
                self.resumo.visto(tarefa);
                self.balao = None;
                self.volta = None;
                ligacao::abrir_colmeia(self.perfil, tarefa, agente, false);
            }
            tela::Pedido::AbrirColmeia => {
                self.volta = None;
                ligacao::abrir_colmeia(self.perfil, 0, None, false);
            }
            tela::Pedido::Desafixar => {
                if !ligacao::desafixar(self.perfil) {
                    eprintln!("não consegui gravar o Desafixar no perfil; a abelha sai mesmo assim");
                }
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
        }
    }
}

impl eframe::App for Fixa {
    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        if self.transparente { [0.0; 4] } else { egui::Rgba::from(tela::tema().fundo).to_array() }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let agora = ui.input(|i| i.time);
        let passou = agora - self.ultimo_quadro;
        self.ultimo_quadro = agora;
        self.quadros += 1;
        // O pedido ao gerenciador de janelas vai com a janela já na tela.
        if self.quadros == 1 {
            ctx.request_repaint();
        } else if self.quadros == 2
            && let Err(e) = fixar::em_todas_as_areas(frame)
        {
            eprintln!("abelha fixa: {e}");
        }
        self.receber(&ctx, agora);
        self.receber_voz(agora);

        let estado = if agora < self.comemorar_ate { Estado::Comemorando } else { self.resumo.estado() };
        if estado != self.estado {
            self.estado = estado;
            self.mudou_em = agora;
            self.animar_ate = agora + ANIMACAO;
        }

        // Onde ela está: a lembrada, ou o canto de baixo à direita.
        let (monitor, fora) = ui.input(|i| (i.viewport().monitor_size, i.viewport().outer_rect));
        let monitor = match monitor.filter(|m| m.x >= 320.0 && m.y >= 240.0) {
            Some(m) => m,
            None => *self.tela.get_or_insert_with(|| {
                let (l, a) = fixar::tamanho_da_tela().unwrap_or((1920, 1080));
                vec2(l as f32, a as f32) / ctx.pixels_per_point()
            }),
        };
        let p = self.posicao.unwrap_or_else(|| pos2(monitor.x - ABELHA - 24.0, monitor.y - ABELHA - 64.0));
        let limitar = |p: Pos2| pos2(p.x.clamp(0.0, (monitor.x - ABELHA).max(0.0)), p.y.clamp(0.0, (monitor.y - ABELHA).max(0.0)));
        let mut p = limitar(p);

        // Com uma janela da Colmeia à vista, a abelha se esconde para não
        // cobrir os botões dela; volta no mesmo lugar quando ela fecha ou
        // minimiza. Falando com a abelha (ou com a resposta no balão), ela fica.
        // Com a Colmeia à vista, você já viu o quadro: o cartão da volta sai.
        if self.janelas > 0 {
            self.volta = None;
        }
        let esconder = self.janelas > 0 && !self.com_voz() && self.arraste.is_none();
        if esconder != self.escondida {
            self.escondida = esconder;
            ctx.send_viewport_cmd(ViewportCommand::Visible(!esconder));
            // Ao voltar, a posição vai de novo (o gerenciador pode ter esquecido).
            self.janela = None;
        }

        // A abelha: clique abre ou fecha o resumo; arrastar (depois de 6 px) move a janela.
        let local = Self::arranjo(p, self.cartao(&ctx), monitor).1;
        let resposta = ui.interact(local, egui::Id::new("abelha"), Sense::click_and_drag()).on_hover_cursor(egui::CursorIcon::Grab);
        let (apertado, origem, ponteiro) = ui.input(|i| (i.pointer.primary_down(), i.pointer.press_origin(), i.pointer.latest_pos()));
        match (self.arraste, apertado, origem, ponteiro, fora) {
            (None, true, Some(o), Some(atual), _) if local.contains(o) && (atual - o).length() > LIMIAR_ARRASTE => {
                self.arraste = Some(o - local.min);
            }
            (Some(pega), ..) => {
                // O ponteiro na tela (pelo X11; sem ele, o da janela): a abelha
                // segue, pega onde foi apertada, até soltar o botão.
                let ppp = ctx.pixels_per_point();
                let tela = fixar::ponteiro().map(|(x, y, b)| (pos2(x, y) / ppp, b)).or_else(|| Some((fora?.min + ponteiro?.to_vec2(), apertado)));
                match tela {
                    Some((global, true)) => {
                        p = limitar(global - pega);
                        // Fora da janela não chegam eventos: o próximo quadro vem daqui.
                        ctx.request_repaint_after(Duration::from_millis(16));
                    }
                    _ => {
                        self.arraste = None;
                        posicao::gravar(self.perfil, p);
                    }
                }
            }
            _ => {}
        }
        self.posicao = Some(p);

        let (janela, abelha, cartao) = Self::arranjo(p, self.cartao(&ctx), monitor);
        self.canto_livre = Self::lado(p, monitor);
        if self.puxando
            && let Some(c) = cartao
        {
            self.puxar(&ctx, c, janela, monitor, (ponteiro, apertado, fora));
        }
        if self.janela != Some(janela) {
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(janela.min));
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(janela.size()));
            self.janela = Some(janela);
        }
        // Com os comandos falados, o clique é para falar e o resumo vai ao botão direito.
        let resposta = if self.arraste.is_some() { resposta.on_hover_cursor(egui::CursorIcon::Grabbing) } else { resposta };
        if resposta.clicked() && self.conversa.is_some() && self.voz.voz_comandos {
            self.falar();
        } else if resposta.clicked() || resposta.secondary_clicked() {
            // Com a conversa ou a tarefa nova abertas, o clique na abelha fecha.
            if self.papo.is_some() || self.nova.is_some() {
                self.papo = None;
                self.nova = None;
            } else {
                self.aberto = !self.aberto;
            }
            self.balao = None;
            self.volta = None;
            self.resposta = None;
        }
        if !self.com_voz() {
            self.acompanhar_papo(&ctx, agora);
        }
        let em_cima = resposta.hovered();
        if em_cima && !self.em_cima {
            self.animar_ate = self.animar_ate.max(agora + ANIMACAO);
        }
        self.em_cima = em_cima;
        let brilho = ctx.animate_bool(egui::Id::new("brilho"), em_cima || self.aberto);
        let animando = agora < self.animar_ate || self.estado == Estado::Comemorando;
        let pintor = ui.painter();
        tela::brilho(pintor, abelha, brilho);
        let lado = ABELHA * 0.7;
        // O microfone aberto fica sempre à vista: a abelha põe o fone (o
        // microfone grande e brilhando ouvindo um comando; discreto, mas
        // aceso, esperando o nome). O "!" do estado vira o indicador do canto.
        let fone = self.conversa.as_ref().and_then(|c| match c.fase() {
            Fase::Abrindo | Fase::Ouvindo => Some(mascote::Fone::Ouvindo),
            _ if c.vigiando() => Some(mascote::Fone::Esperando),
            _ => None,
        });
        let acessorios = mascote::Acessorios { fone, sem_aviso: true };
        mascote::desenhar_com(pintor, abelha.center() + vec2(0.0, 2.0), lado, self.modelo, self.estado, self.tempo(agora, animando), acessorios);
        tela::indicador(pintor, abelha, self.resumo.contagem());

        let mut pedido = None;
        if let Some(rect) = cartao {
            if self.com_voz() {
                self.balao_voz(ui, rect);
            } else if self.papo.is_some() {
                self.mostrar_papo(ui, rect, agora);
            } else if self.nova.is_some() {
                self.mostrar_nova(ui, rect);
            } else if self.aberto {
                let destaques = self.resumo.destaques();
                let rascunho = if self.conversa.is_some() { Some(&mut self.rascunho) } else { None };
                pedido = tela::resumo(ui, rect, &self.nome, self.ligada, self.resumo.contagem(), &destaques, rascunho);
            } else if let Some(v) = &self.volta {
                pedido = tela::volta(ui, rect, &self.nome, v.fora, &v.avisos);
            } else if let Some(b) = &mut self.balao {
                // O tempo do balão só corre sem o mouse em cima.
                if !ui.rect_contains_pointer(rect) {
                    b.resta -= passou.min(0.25);
                }
                pedido = tela::balao(ui, rect, &self.nome, &b.aviso, (b.resta / BALAO) as f32);
                if b.resta <= 0.0 {
                    self.balao = None;
                    // Mais um quadro: sem ele a janela ficava com o balão vazio desenhado.
                    ctx.request_repaint();
                }
            }
        }
        if let Some(p) = pedido {
            self.atender(&ctx, p);
        }
        // Com um cartão à vista, o clique nele traz o foco do teclado (sem o
        // foco, a tecla não chega) e o Esc faz o que o "x" do cartão faz.
        // Na voz, não: o balão dela tem o próprio ritmo.
        if self.cartao(&ctx).is_some() && !self.com_voz() {
            let (clicou, com_foco) = ui.input(|i| (i.pointer.any_pressed(), i.viewport().focused.unwrap_or(false)));
            if clicou && !com_foco {
                ctx.send_viewport_cmd(ViewportCommand::Focus);
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                if self.papo.is_some() {
                    // Na conversa, o Esc fecha a abelha (ela continua fixa).
                    self.papo = None;
                    self.atender(&ctx, tela::Pedido::Fechar);
                } else if self.nova.is_some() {
                    self.nova = None;
                    self.aberto = true;
                } else {
                    self.atender(&ctx, tela::Pedido::Fechar);
                }
            }
        }

        // Só pede quadros enquanto algo se mexe; parada, dorme até um evento.
        if animando {
            ctx.request_repaint_after(Duration::from_secs_f64(1.0 / QUADROS_ANIMACAO));
        } else if self.balao.is_some() && !self.aberto && self.volta.is_none() && !self.com_voz() {
            ctx.request_repaint_after(Duration::from_secs_f64(1.0 / QUADROS_BALAO));
        }
        // A resposta da voz some sozinha: um quadro só, na hora.
        if let Some(r) = &self.resposta {
            ctx.request_repaint_after(Duration::from_secs_f64((r.ate - agora).max(0.05)));
        }
    }
}

fn perfil_da_linha() -> Option<i64> {
    let mut argumentos = std::env::args().skip(1);
    while let Some(a) = argumentos.next() {
        if a == "--perfil" {
            return argumentos.next()?.parse().ok().filter(|n: &i64| *n > 0);
        }
    }
    None
}

/// Os visuais do egui (caixa de texto, seleção) acompanham o tema da abelha.
fn aplicar_visual(ctx: &egui::Context) {
    ctx.set_visuals(if tela::tema().clara { egui::Visuals::light() } else { egui::Visuals::dark() });
}

fn main() -> eframe::Result {
    let Some(perfil) = perfil_da_linha() else {
        eprintln!("uso: colmeia-abelha --perfil N");
        std::process::exit(2);
    };
    let Some(trava) = travar(perfil) else {
        // Já há uma abelha deste perfil na tela.
        return Ok(());
    };
    let posicao = posicao::ler(perfil);
    let transparente = fixar::tem_compositor();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Colmeia")
        .with_app_id("colmeia-abelha")
        .with_inner_size([ABELHA, ABELHA])
        .with_decorations(false)
        .with_transparent(transparente)
        .with_resizable(false)
        .with_taskbar(false)
        .with_window_level(egui::WindowLevel::AlwaysOnTop);
    // No X11, uma "dock": o "mostrar a área de trabalho" (Super+D) não a
    // esconde, ela não vai para a barra de tarefas nem para o Alt+Tab, e fica
    // em todas as áreas de trabalho. O tipo precisa estar na janela antes de
    // ela aparecer (depois, o gerenciador não relê).
    #[cfg(target_os = "linux")]
    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        viewport = viewport.with_window_type(egui::X11WindowType::Dock);
    }
    if let Some(p) = posicao {
        viewport = viewport.with_position(p);
    }
    let opcoes = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native(
        "colmeia-abelha",
        opcoes,
        Box::new(move |cc| {
            // O tema da abelha é dela, guardado ao lado da posição.
            tela::escolher_tema(posicao::ler_tema(perfil).and_then(|n| tela::tema_chamado(&n)).unwrap_or(0));
            aplicar_visual(&cc.egui_ctx);
            // Ctrl + e Ctrl − são da letra da conversa, não do zoom da janela inteira.
            cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
            tela::instalar_fontes(&cc.egui_ctx);
            let (envio, recebe) = mpsc::channel();
            ligacao::iniciar(perfil, cc.egui_ctx.clone(), envio.clone());
            let pausa = std::env::var("COLMEIA_PAUSA").ok().and_then(|s| s.parse().ok()).map_or(PAUSA, Duration::from_secs);
            vigiar_pausa(pausa, cc.egui_ctx.clone(), envio.clone());
            Ok(Box::new(Fixa {
                perfil,
                modelo: Modelo::default(),
                nome: "Colmeia".into(),
                resumo: Resumo::default(),
                recebe,
                ligada: false,
                estado: Estado::Dormindo,
                mudou_em: 0.0,
                animar_ate: ANIMACAO,
                comemorar_ate: 0.0,
                em_cima: false,
                arraste: None,
                janelas: 0,
                escondida: false,
                aberto: false,
                balao: None,
                volta: None,
                posicao,
                janela: None,
                ultimo_quadro: 0.0,
                quadros: 0,
                tela: None,
                transparente,
                voz: OpcoesVoz::default(),
                conversa: None,
                resposta: None,
                rascunho: String::new(),
                atalho: None,
                papo: None,
                nova: None,
                tamanho_papo: posicao::ler_tamanho(perfil).unwrap_or(tela::PAPO_MINIMO).max(tela::PAPO_MINIMO),
                puxando: false,
                letra_papo: posicao::ler_letra(perfil).filter(|l| tela::LETRAS.contains(l)).unwrap_or(1.0),
                canto_livre: (true, true),
                envio,
                _trava: trava,
            }))
        }),
    )
}
