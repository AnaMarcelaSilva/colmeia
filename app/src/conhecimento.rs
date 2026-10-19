//! A base de conhecimento do workspace: as notas que todo agente recebe ao
//! começar, as do workspace inteiro e as de cada projeto, e as sugestões dos
//! agentes esperando você. A tela só mostra e pede; quem valida, guarda e
//! decide o que vai ao agente é o núcleo. A lista vem numa thread e volta a
//! ser lida quando um evento diz que a base deste workspace mudou.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

use eframe::egui::{self, CornerRadius, RichText, Stroke, vec2};

use crate::api::{self, ArquivoConhecimento, Conhecimento};
use crate::registro;
use crate::tema::{self, Icone, cores};

/// O que o agente recebe ao começar, em caracteres (o mesmo limite do núcleo).
const LIMITE_INICIO: usize = 8000;
const MAX_TITULO: usize = 80;
const MAX_TEXTO: usize = 4000;
const LARGURA: f32 = 760.0;
const MAX_ARQUIVOS: usize = 10;
/// Fechada, a nota mostra estas linhas do texto.
const LINHAS_RESUMO: usize = 3;

enum Mensagem {
    Lista {
        workspace: i64,
        resultado: Result<Vec<Conhecimento>, String>,
    },
    /// Uma mudança pedida ao núcleo terminou; o evento traz a lista nova.
    Feito(Result<(), String>),
    /// A nota foi salva, mas algum arquivo não: o editor fecha e o aviso fica.
    Parcial(String),
    /// Arquivos escolhidos no seletor do sistema.
    Escolhidos(Vec<PathBuf>),
}

/// A nota sendo escrita: nova (`id` 0, no `projeto` dado, 0 para o
/// workspace) ou uma existente, sugestão inclusive.
struct Edicao {
    id: i64,
    projeto: i64,
    titulo: String,
    texto: String,
    /// Os que a nota já tem e continuam, os que saem ao salvar e os do disco
    /// que entram.
    arquivos: Vec<ArquivoConhecimento>,
    tirar: Vec<i64>,
    novos: Vec<PathBuf>,
}

impl Edicao {
    fn nova(projeto: i64) -> Self {
        Edicao { id: 0, projeto, titulo: String::new(), texto: String::new(), arquivos: Vec::new(), tirar: Vec::new(), novos: Vec::new() }
    }

    fn de(n: &Conhecimento) -> Self {
        Edicao {
            id: n.id,
            projeto: n.projeto_id,
            titulo: n.titulo.clone(),
            texto: n.texto.clone(),
            arquivos: n.arquivos.clone(),
            tirar: Vec::new(),
            novos: Vec::new(),
        }
    }

    fn quantos_arquivos(&self) -> usize {
        self.arquivos.len() + self.novos.len()
    }

    /// Junta os arquivos do disco, sem repetir e até o limite da nota.
    fn juntar(&mut self, caminhos: Vec<PathBuf>) -> Option<String> {
        let mut aviso = None;
        for c in caminhos {
            if self.novos.contains(&c) {
                continue;
            }
            if self.quantos_arquivos() >= MAX_ARQUIVOS {
                aviso = Some(format!("Uma nota leva até {MAX_ARQUIVOS} arquivos."));
                break;
            }
            match std::fs::metadata(&c) {
                Ok(m) if m.len() > api::MAX_ARQUIVO_CONHECIMENTO => {
                    aviso = Some(format!("{} passa de {} MB e ficou de fora.", nome_do(&c), api::MAX_ARQUIVO_CONHECIMENTO >> 20))
                }
                Ok(m) if m.len() == 0 => aviso = Some(format!("{} está vazio e ficou de fora.", nome_do(&c))),
                Ok(_) => self.novos.push(c),
                Err(_) => aviso = Some(format!("Não consegui ler {}.", nome_do(&c))),
            }
        }
        aviso
    }
}

fn nome_do(caminho: &std::path::Path) -> String {
    caminho.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// O que a tela precisa saber do workspace neste quadro.
pub struct Contexto<'a> {
    pub workspace: i64,
    pub nome: &'a str,
    /// Os projetos do workspace: id e nome.
    pub projetos: Vec<(i64, &'a str)>,
    pub automatico: bool,
    /// Muda a cada evento da base de conhecimento.
    pub versao: u64,
    pub pode: bool,
}

pub struct TelaConhecimento {
    workspace: i64,
    notas: Option<Result<Vec<Conhecimento>, String>>,
    versao_lida: Option<u64>,
    edicao: Option<Edicao>,
    /// As notas abertas por inteiro; as outras mostram o começo.
    abertas: HashSet<i64>,
    escolhendo: bool,
    remover: Option<i64>,
    erro: Option<String>,
    ocupado: bool,
    envio: Sender<Mensagem>,
    chegando: Receiver<Mensagem>,
}

impl Default for TelaConhecimento {
    fn default() -> Self {
        let (envio, chegando) = mpsc::channel();
        TelaConhecimento {
            workspace: 0,
            notas: None,
            versao_lida: None,
            edicao: None,
            abertas: HashSet::new(),
            escolhendo: false,
            remover: None,
            erro: None,
            ocupado: false,
            envio,
            chegando,
        }
    }
}

impl TelaConhecimento {
    /// Abre a base de outro workspace (a do mesmo fica como estava).
    pub fn abrir(&mut self, workspace: i64) {
        if self.workspace != workspace {
            self.workspace = workspace;
            self.notas = None;
            self.edicao = None;
            self.abertas.clear();
            self.remover = None;
            self.erro = None;
        }
        self.versao_lida = None;
    }

    fn receber(&mut self) {
        while let Ok(m) = self.chegando.try_recv() {
            match m {
                Mensagem::Lista { workspace, resultado } if workspace == self.workspace => self.notas = Some(resultado),
                Mensagem::Lista { .. } => {}
                Mensagem::Feito(resultado) => {
                    self.ocupado = false;
                    match resultado {
                        Ok(()) => {
                            self.edicao = None;
                            self.remover = None;
                            self.erro = None;
                        }
                        Err(e) => self.erro = Some(e),
                    }
                }
                Mensagem::Parcial(aviso) => {
                    self.ocupado = false;
                    self.edicao = None;
                    self.erro = Some(aviso);
                }
                Mensagem::Escolhidos(caminhos) => {
                    self.escolhendo = false;
                    if let Some(e) = self.edicao.as_mut() {
                        self.erro = e.juntar(caminhos);
                    }
                }
            }
        }
    }

    fn pedir(&mut self, ctx: &egui::Context, pedido: impl FnOnce() -> Result<(), String> + Send + 'static) {
        self.ocupado = true;
        self.erro = None;
        registro::em_segundo_plano(&self.envio, ctx, move || Mensagem::Feito(pedido()));
    }

    pub fn mostrar(&mut self, ui: &mut egui::Ui, cx: &Contexto) {
        self.receber();
        self.soltos(ui.ctx(), cx);
        if self.versao_lida != Some(cx.versao) {
            self.versao_lida = Some(cx.versao);
            let workspace = self.workspace;
            registro::em_segundo_plano(&self.envio, ui.ctx(), move || Mensagem::Lista { workspace, resultado: api::conhecimento(workspace) });
        }
        egui::ScrollArea::vertical().id_salt("conhecimento").auto_shrink([false, false]).show(ui, |ui| {
            let largura = ui.available_width().min(LARGURA);
            let sobra = (ui.available_width() - largura) / 2.0;
            ui.horizontal(|ui| {
                ui.add_space(sobra.max(0.0));
                ui.vertical(|ui| {
                    ui.set_width(largura);
                    ui.add_space(8.0);
                    self.conteudo(ui, cx);
                    ui.add_space(24.0);
                });
            });
        });
    }

    /// Arquivos soltos na página vão para a nota sendo escrita; sem nenhuma
    /// aberta, começam uma nota do workspace com eles.
    fn soltos(&mut self, ctx: &egui::Context, cx: &Contexto) {
        let soltos: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|c| c.is_file()).collect());
        if soltos.is_empty() || !cx.pode || self.ocupado || !matches!(self.notas, Some(Ok(_))) {
            return;
        }
        let e = self.edicao.get_or_insert_with(|| {
            let mut nova = Edicao::nova(0);
            nova.titulo = soltos[0].file_stem().map(|n| n.to_string_lossy().chars().take(MAX_TITULO).collect()).unwrap_or_default();
            nova
        });
        self.erro = e.juntar(soltos);
    }

    fn escolher_arquivos(&mut self, ctx: &egui::Context) {
        self.escolhendo = true;
        registro::em_segundo_plano(&self.envio, ctx, || {
            Mensagem::Escolhidos(rfd::FileDialog::new().set_title("Arquivos da nota").pick_files().unwrap_or_default())
        });
    }

    fn conteudo(&mut self, ui: &mut egui::Ui, cx: &Contexto) {
        let p = cores();
        tema::cabecalho(
            ui,
            "Conhecimento",
            &format!(
                "Todo agente que começa em {} já recebe estas notas, sem você repetir. Os agentes também sugerem notas, que só valem quando você aprova.",
                cx.nome
            ),
        );
        ui.add_space(14.0);
        let notas = match &self.notas {
            None => {
                for _ in 0..3 {
                    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 72.0), egui::Sense::hover());
                    tema::esqueleto(ui.painter(), r, 12);
                    ui.add_space(8.0);
                }
                return;
            }
            Some(Err(e)) => {
                ui.label(RichText::new(format!("Não consegui ler a base de conhecimento: {e}")).color(p.erro));
                return;
            }
            Some(Ok(notas)) => notas.clone(),
        };
        if let Some(e) = &self.erro {
            ui.label(RichText::new(e).color(p.erro).size(13.0));
            ui.add_space(8.0);
        }
        if cx.pode && ui.ctx().input(|i| !i.raw.hovered_files.is_empty()) {
            let aviso =
                if self.edicao.is_some() { "Solte para anexar à nota que você está escrevendo" } else { "Solte para começar uma nota com estes arquivos" };
            egui::Frame::new()
                .fill(tema::misturar(p.superficie, p.destaque, 0.12))
                .stroke(Stroke::new(1.5, p.destaque))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(egui::Margin::same(14))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(aviso).color(p.texto).size(13.5));
                });
            ui.add_space(10.0);
        }

        let aprovadas: Vec<&Conhecimento> = notas.iter().filter(|n| !n.sugerida()).collect();
        let sugeridas: Vec<&Conhecimento> = notas.iter().filter(|n| n.sugerida()).collect();
        if aprovadas.is_empty() && sugeridas.is_empty() && self.edicao.is_none() {
            self.vazio(ui, cx);
            self.rodape(ui, cx);
            return;
        }

        if !sugeridas.is_empty() {
            titulo_da_secao(
                ui,
                &format!("SUGESTÕES DOS AGENTES · {}", sugeridas.len()),
                "Só valem depois que você aprova. Leia antes, porque um agente pode ter lido algo errado.",
                true,
            );
            for n in &sugeridas {
                let alvo = notas.iter().find(|a| a.id == n.substitui).map(|a| a.titulo.as_str());
                self.cartao(ui, n, cx, alvo);
            }
            ui.add_space(12.0);
        }

        let tamanho = |projeto: i64| -> usize {
            aprovadas.iter().filter(|n| n.projeto_id == 0 || n.projeto_id == projeto).map(|n| n.titulo.chars().count() + n.texto.chars().count() + 30).sum()
        };
        let secoes: Vec<(i64, String, String)> = std::iter::once((0, "TODO O WORKSPACE".to_string(), "Vale para todos os projetos daqui.".to_string()))
            .chain(cx.projetos.iter().map(|(id, nome)| {
                let usado = tamanho(*id);
                let medida = if usado > LIMITE_INICIO {
                    format!("Um agente aqui recebe as primeiras notas até {} caracteres; as outras ele lê quando precisar.", milhar(LIMITE_INICIO))
                } else {
                    format!("Um agente aqui recebe {} de {} caracteres.", milhar(usado), milhar(LIMITE_INICIO))
                };
                (*id, nome.to_uppercase(), medida)
            }))
            .collect();
        for (projeto, titulo, explicacao) in secoes {
            let da_secao: Vec<&&Conhecimento> = aprovadas.iter().filter(|n| n.projeto_id == projeto).collect();
            let escrevendo_nova = self.edicao.as_ref().is_some_and(|e| e.id == 0 && e.projeto == projeto);
            if projeto != 0 && da_secao.is_empty() && !escrevendo_nova {
                // Projeto sem notas: uma linha só, para a página não crescer à toa.
                ui.horizontal(|ui| {
                    ui.label(texto_secao(&titulo));
                    ui.label(RichText::new("sem notas próprias").color(p.suave).size(12.0));
                    if cx.pode && self.edicao.is_none() && tema::link_com(ui, "Anotar", true).clicked() {
                        self.edicao = Some(Edicao::nova(projeto));
                    }
                });
                ui.add_space(10.0);
                continue;
            }
            titulo_da_secao(ui, &titulo, &explicacao, false);
            for n in da_secao {
                self.cartao(ui, n, cx, None);
            }
            if escrevendo_nova {
                self.editor(ui, cx);
            } else if cx.pode && self.edicao.is_none() && tema::link_com(ui, "+ Nova nota", true).clicked() {
                self.edicao = Some(Edicao::nova(projeto));
            }
            ui.add_space(14.0);
        }
        self.rodape(ui, cx);
    }

    /// Sem nota nenhuma: o que é a base, com exemplos, e o primeiro passo.
    fn vazio(&mut self, ui: &mut egui::Ui, cx: &Contexto) {
        let p = cores();
        egui::Frame::new().fill(p.superficie).corner_radius(CornerRadius::same(14)).inner_margin(egui::Margin::same(22)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical_centered(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(44.0, 44.0), egui::Sense::hover());
                ui.painter().circle_filled(r.center(), 22.0, tema::misturar(p.superficie, p.destaque, 0.15));
                tema::desenhar_icone(ui.painter(), r.center(), Icone::Livro, p.destaque);
                ui.add_space(10.0);
                ui.label(tema::texto_forte("Nada anotado ainda", 15.5).color(p.texto));
                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "Escreva uma vez o que você explica para todo agente, como \"as pastas com números são os logs de cada aparelho\", \"o nome do arquivo sai da data e do código da loja\" ou \"sempre olhe também o projeto da API\".",
                    )
                    .color(p.suave)
                    .size(13.0),
                );
                ui.add_space(14.0);
                if cx.pode && tema::botao_principal(ui, "Escrever a primeira nota", true).clicked() {
                    self.edicao = Some(Edicao::nova(0));
                }
            });
        });
        if self.edicao.is_some() {
            ui.add_space(12.0);
            self.editor(ui, cx);
        }
        ui.add_space(14.0);
    }

    /// Uma nota: o título, o texto e as ações. A sugestão diz de onde veio e
    /// o que muda, com aprovar, editar e recusar.
    fn cartao(&mut self, ui: &mut egui::Ui, n: &Conhecimento, cx: &Contexto, muda: Option<&str>) {
        if self.edicao.as_ref().is_some_and(|e| e.id == n.id) {
            self.editor(ui, cx);
            return;
        }
        let p = cores();
        let sugerida = n.sugerida();
        let borda = if sugerida { tema::misturar(p.superficie, p.alerta, 0.55) } else { p.borda };
        egui::Frame::new()
            .fill(p.superficie)
            .stroke(Stroke::new(1.0, borda))
            .corner_radius(CornerRadius::same(12))
            .inner_margin(egui::Margin { left: 16, right: 10, top: 10, bottom: 12 })
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(tema::texto_forte(&n.titulo, 14.0).color(p.texto));
                    if sugerida {
                        let onde = match n.projeto_id {
                            0 => "para todo o workspace".to_string(),
                            id => cx.projetos.iter().find(|(p, _)| *p == id).map_or_else(String::new, |(_, nome)| format!("para {nome}")),
                        };
                        let de = match muda {
                            Some(titulo) => format!("muda a nota \"{titulo}\" {onde}"),
                            None => format!("nota nova {onde}"),
                        };
                        ui.label(RichText::new(de).color(p.suave).size(12.0));
                    }
                    if cx.pode && !sugerida && self.edicao.is_none() {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if tema::botao_icone(ui, Icone::Apagar, 26.0).on_hover_text("Apagar a nota").clicked() {
                                self.remover = Some(n.id);
                            }
                            if tema::botao_icone(ui, Icone::Lapis, 26.0).on_hover_text("Editar").clicked() {
                                self.edicao = Some(Edicao::de(n));
                            }
                        });
                    }
                });
                ui.add_space(4.0);
                // Fechada, a nota mostra o começo; a sugestão vem inteira,
                // porque é para ler antes de aprovar.
                let aberta = sugerida || self.abertas.contains(&n.id);
                let texto = ui.painter().layout_job(texto_da_nota(&n.texto, ui.available_width(), (!aberta).then_some(LINHAS_RESUMO)));
                let cortado = texto.elided;
                let resposta = ui.add(egui::Label::new(texto).sense(if sugerida { egui::Sense::hover() } else { egui::Sense::click() }));
                let mut trocar = !sugerida && resposta.clone().on_hover_cursor(egui::CursorIcon::PointingHand).clicked();
                if !sugerida && (cortado || self.abertas.contains(&n.id)) {
                    trocar |= tema::link_com(ui, if cortado { "Ver tudo" } else { "Mostrar menos" }, true).clicked();
                }
                if trocar && !self.abertas.remove(&n.id) {
                    self.abertas.insert(n.id);
                }
                if !n.arquivos.is_empty() {
                    ui.add_space(6.0);
                    self.lista_de_arquivos(ui, &n.arquivos);
                }
                if self.remover == Some(n.id) {
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Apagar esta nota? Os próximos agentes não vão mais recebê-la.").color(p.texto).size(13.0));
                        if tema::botao_alerta(ui, "Apagar").clicked() && !self.ocupado {
                            let nota = n.id;
                            self.pedir(ui.ctx(), move || api::remover_conhecimento(nota));
                        }
                        if tema::botao_secundario(ui, "Cancelar").clicked() {
                            self.remover = None;
                        }
                    });
                }
                if sugerida && cx.pode {
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if tema::botao_principal(ui, "Aprovar", !self.ocupado).clicked() {
                            let nota = n.id;
                            self.pedir(ui.ctx(), move || api::aprovar_conhecimento(nota).map(|_| ()));
                        }
                        if tema::botao_secundario_com(ui, "Editar antes", self.edicao.is_none()).clicked() {
                            self.edicao = Some(Edicao::de(n));
                        }
                        if tema::botao_secundario_com(ui, "Recusar", !self.ocupado).clicked() {
                            let nota = n.id;
                            self.pedir(ui.ctx(), move || api::remover_conhecimento(nota));
                        }
                    });
                }
            });
        ui.add_space(8.0);
    }

    /// O título e o texto da nota sendo escrita, com salvar e cancelar.
    fn editor(&mut self, ui: &mut egui::Ui, cx: &Contexto) {
        let p = cores();
        let Some(e) = self.edicao.as_mut() else { return };
        let nova = e.id == 0;
        let mut salvar = false;
        let mut cancelar = false;
        let mut pedir_seletor = false;
        egui::Frame::new()
            .fill(p.superficie)
            .stroke(Stroke::new(1.5, p.destaque))
            .corner_radius(CornerRadius::same(12))
            .inner_margin(egui::Margin::same(16))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let titulo = tema::campo(ui, "Título", &mut e.titulo, "Ex.: Onde ficam os logs");
                if nova && e.titulo.is_empty() && e.texto.is_empty() {
                    titulo.request_focus();
                }
                ui.add_space(8.0);
                ui.label(RichText::new("O que o agente precisa saber").color(p.suave).size(12.5));
                ui.add_space(2.0);
                tema::campo_multilinha(ui, &mut e.texto, 6, 320.0, ui.id().with(("texto-nota", e.id, e.projeto)), false);
                let (titulo_n, texto_n) = (e.titulo.trim().chars().count(), e.texto.trim().chars().count());
                ui.add_space(4.0);
                let excesso = titulo_n > MAX_TITULO || texto_n > MAX_TEXTO;
                ui.label(
                    RichText::new(format!("{} de {} caracteres", milhar(texto_n), milhar(MAX_TEXTO))).color(if excesso { p.erro } else { p.suave }).size(11.5),
                );

                // Os arquivos: o agente recebe o caminho e lê quando precisar.
                ui.add_space(10.0);
                ui.label(RichText::new("Arquivos").color(p.suave).size(12.5));
                ui.add_space(2.0);
                let mut tirar = None;
                let mut tirar_novo = None;
                for a in &e.arquivos {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&a.nome).color(p.texto).size(13.0));
                        ui.label(RichText::new(tamanho(a.bytes as u64)).color(p.suave).size(11.5));
                        if tema::botao_icone(ui, Icone::Fechar, 22.0).on_hover_text("Tirar da nota").clicked() {
                            tirar = Some(a.id);
                        }
                    });
                }
                for (i, c) in e.novos.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(nome_do(c)).color(p.texto).size(13.0));
                        let bytes = std::fs::metadata(c).map(|m| m.len()).unwrap_or(0);
                        ui.label(RichText::new(format!("{} · entra ao salvar", tamanho(bytes))).color(p.suave).size(11.5));
                        if tema::botao_icone(ui, Icone::Fechar, 22.0).on_hover_text("Tirar").clicked() {
                            tirar_novo = Some(i);
                        }
                    });
                }
                if let Some(id) = tirar {
                    e.arquivos.retain(|a| a.id != id);
                    e.tirar.push(id);
                }
                if let Some(i) = tirar_novo {
                    e.novos.remove(i);
                }
                let mut escolher = false;
                ui.horizontal(|ui| {
                    let cabe = e.quantos_arquivos() < MAX_ARQUIVOS;
                    escolher =
                        tema::botao_secundario_com(ui, if self.escolhendo { "Escolhendo…" } else { "Anexar arquivo" }, cabe && !self.escolhendo).clicked();
                    let dica = if cabe {
                        "ou arraste para cá. PDF, planilha, modelo, o que o agente precisar ler."
                    } else {
                        "Esta nota já tem o máximo de arquivos."
                    };
                    ui.label(RichText::new(dica).color(p.suave).size(12.0));
                });
                pedir_seletor = escolher;
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let pronto = titulo_n > 0 && texto_n > 0 && !excesso && !self.ocupado;
                    salvar = tema::botao_principal(ui, "Salvar", pronto).clicked();
                    cancelar = tema::botao_secundario(ui, "Cancelar").clicked();
                });
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    cancelar = true;
                }
            });
        ui.add_space(8.0);
        if pedir_seletor && !cancelar {
            self.escolher_arquivos(ui.ctx());
        }
        if cancelar {
            self.edicao = None;
            self.erro = None;
        } else if salvar && let Some(e) = &self.edicao {
            let (id, projeto, titulo, texto, workspace) = (e.id, e.projeto, e.titulo.trim().to_string(), e.texto.trim().to_string(), cx.workspace);
            let (tirar, novos) = (e.tirar.clone(), e.novos.clone());
            self.ocupado = true;
            self.erro = None;
            registro::em_segundo_plano(&self.envio, ui.ctx(), move || {
                let salva = if id == 0 { api::criar_conhecimento(workspace, projeto, &titulo, &texto) } else { api::editar_conhecimento(id, &titulo, &texto) };
                let nota = match salva {
                    Ok(n) => n.id,
                    Err(e) => return Mensagem::Feito(Err(e)),
                };
                // A nota já está salva: o que falhar daqui em diante vira aviso.
                let falhas: Vec<String> = tirar
                    .into_iter()
                    .filter_map(|a| api::remover_arquivo_conhecimento(a).err())
                    .chain(novos.iter().filter_map(|c| api::anexar_ao_conhecimento(nota, c).err()))
                    .collect();
                if falhas.is_empty() { Mensagem::Feito(Ok(())) } else { Mensagem::Parcial(format!("A nota foi salva, mas {}", falhas.join("; "))) }
            });
        }
    }

    /// Os arquivos da nota; o clique abre o arquivo (ou a pasta dele, para os
    /// tipos que o sistema poderia executar).
    fn lista_de_arquivos(&mut self, ui: &mut egui::Ui, arquivos: &[ArquivoConhecimento]) {
        let p = cores();
        ui.horizontal_wrapped(|ui| {
            for a in arquivos {
                let resposta = tema::link_com(ui, &a.nome, true).on_hover_text(format!("{} · {}", tamanho(a.bytes as u64), a.caminho));
                if resposta.clicked() {
                    let aberto = if crate::sistema::abre_no_sistema(&a.nome) {
                        crate::sistema::abrir_no_sistema(&a.caminho)
                    } else {
                        crate::sistema::abrir_pasta(std::path::Path::new(&a.caminho).parent().and_then(|d| d.to_str()).unwrap_or_default())
                    };
                    if let Err(e) = aberto {
                        self.erro = Some(format!("Não consegui abrir {}: {e}", a.nome));
                    }
                }
                ui.label(RichText::new(tamanho(a.bytes as u64)).color(p.suave).size(11.5));
                ui.add_space(8.0);
            }
        });
    }

    /// A aprovação automática, com o porquê de ela nascer desligada.
    fn rodape(&mut self, ui: &mut egui::Ui, cx: &Contexto) {
        let p = cores();
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), egui::Sense::hover());
        ui.painter().hline(r.x_range(), r.center().y, Stroke::new(1.0, p.borda));
        ui.add_space(12.0);
        let mut automatico = cx.automatico;
        if tema::caixa_marcar_com(ui, "Aceitar na hora o que os agentes anotarem", &mut automatico, cx.pode && !self.ocupado).clicked() && cx.pode {
            let workspace = cx.workspace;
            self.pedir(ui.ctx(), move || api::conhecimento_automatico(workspace, automatico).map(|_| ()));
        }
        ui.label(
            RichText::new("Desligado, a nota de um agente espera você aqui. É mais seguro, porque um agente que leu um arquivo com instruções escondidas não consegue ensiná-las aos próximos.")
                .color(p.suave)
                .size(12.0),
        );
    }
}

fn texto_secao(texto: &str) -> RichText {
    RichText::new(texto).font(tema::forte(10.5)).color(cores().suave)
}

fn titulo_da_secao(ui: &mut egui::Ui, titulo: &str, explicacao: &str, alerta: bool) {
    let p = cores();
    ui.label(texto_secao(titulo).color(if alerta { p.alerta } else { p.suave }));
    ui.label(RichText::new(explicacao).color(p.suave).size(12.0));
    ui.add_space(6.0);
}

/// 8000 vira "8.000".
/// O texto da nota com o pouco de markdown que as notas usam: **negrito** e
/// `código`. Fechada (`linhas`), as linhas em branco somem e o resto corta
/// com reticências.
fn texto_da_nota(texto: &str, largura: f32, linhas: Option<usize>) -> egui::text::LayoutJob {
    let p = cores();
    let normal = egui::TextFormat::simple(egui::FontId::proportional(13.5), p.texto);
    let negrito = egui::TextFormat::simple(tema::forte(13.5), p.texto);
    let codigo =
        egui::TextFormat { background: tema::misturar(p.superficie, p.texto, 0.08), ..egui::TextFormat::simple(egui::FontId::monospace(12.5), p.texto) };
    let mut job = egui::text::LayoutJob::default();
    let compacto: String;
    let texto = match linhas {
        Some(_) => {
            compacto = texto.lines().filter(|l| !l.trim().is_empty()).collect::<Vec<_>>().join("\n");
            compacto.as_str()
        }
        None => texto,
    };
    for (i, trecho) in texto.split("**").enumerate() {
        let forte = i % 2 == 1;
        for (j, parte) in trecho.split('`').enumerate() {
            let formato = if j % 2 == 1 {
                codigo.clone()
            } else if forte {
                negrito.clone()
            } else {
                normal.clone()
            };
            if !parte.is_empty() {
                job.append(parte, 0.0, formato);
            }
        }
    }
    job.wrap.max_width = largura;
    if let Some(n) = linhas {
        job.wrap.max_rows = n;
    }
    job
}

/// "820 KB", "3,4 MB".
fn tamanho(bytes: u64) -> String {
    if bytes < 1 << 20 {
        format!("{} KB", (bytes as f64 / 1024.0).ceil() as u64)
    } else {
        format!("{:.1} MB", bytes as f64 / (1 << 20) as f64).replace('.', ",")
    }
}

fn milhar(n: usize) -> String {
    let texto = n.to_string();
    let mut saida = String::new();
    for (i, c) in texto.chars().enumerate() {
        if i > 0 && (texto.len() - i).is_multiple_of(3) {
            saida.push('.');
        }
        saida.push(c);
    }
    saida
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn milhar_com_ponto() {
        assert_eq!(milhar(0), "0");
        assert_eq!(milhar(999), "999");
        assert_eq!(milhar(8000), "8.000");
        assert_eq!(milhar(1234567), "1.234.567");
    }

    #[test]
    fn tamanho_em_kb_e_mb() {
        assert_eq!(tamanho(1), "1 KB");
        assert_eq!(tamanho(820 * 1024), "820 KB");
        assert_eq!(tamanho(3_565_158), "3,4 MB");
    }

    #[test]
    fn nota_junta_arquivos_sem_repetir_e_ate_o_limite() {
        let pasta = std::env::temp_dir().join(format!("colmeia-teste-nota-{}", std::process::id()));
        std::fs::create_dir_all(&pasta).unwrap();
        let caminhos: Vec<PathBuf> = (0..12)
            .map(|i| {
                let c = pasta.join(format!("{i}.txt"));
                std::fs::write(&c, "x").unwrap();
                c
            })
            .collect();
        let vazio = pasta.join("vazio.txt");
        std::fs::write(&vazio, "").unwrap();
        let mut e = Edicao::nova(0);
        assert!(e.juntar(vec![caminhos[0].clone(), caminhos[0].clone(), vazio]).is_some_and(|a| a.contains("vazio")));
        assert_eq!(e.novos.len(), 1);
        assert!(e.juntar(caminhos.clone()).is_some_and(|a| a.contains("até 10")));
        assert_eq!(e.novos.len(), MAX_ARQUIVOS);
        std::fs::remove_dir_all(&pasta).unwrap();
    }

    #[test]
    fn nota_com_negrito_e_codigo_sem_os_simbolos() {
        let job = texto_da_nota("**Comandos**\n\nRode `make teste`.", 400.0, Some(3));
        assert_eq!(job.text, "Comandos\nRode make teste.");
        assert_eq!(job.sections.len(), 4);
        assert_eq!(job.wrap.max_rows, 3);
        assert_eq!(texto_da_nota("a\n\nb", 400.0, None).text, "a\n\nb");
    }
}
