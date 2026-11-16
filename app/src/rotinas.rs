//! As rotinas do workspace: pedidos guardados que o Claude Code responde em
//! texto a partir do quadro de um período e dos commits dos projetos, quando
//! você manda ou no horário da agenda. A tela é uma tabela, uma linha por
//! rotina, e uma gaveta à direita com o texto da última rodada (ou o editor).
//! Quem roda, agenda e guarda é o núcleo; a tela só mostra e pede, e busca a
//! lista de novo quando um evento diz que algo mudou.
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use eframe::egui::{self, CornerRadius, RichText, Sense, Stroke, vec2};

use crate::api::{self, NovaRotina, Rodada, Rotina};
use crate::registro;
use crate::tema::{self, Icone, cores};

const LARGURA_GAVETA: f32 = 440.0;
const MAX_NOME: usize = 80;
const MAX_PEDIDO: usize = 2000;
const DIAS: [&str; 7] = ["Seg", "Ter", "Qua", "Qui", "Sex", "Sáb", "Dom"];
const PERIODOS: [(&str, i64); 5] = [("Últimos 7 dias", 7), ("14 dias", 14), ("30 dias", 30), ("Desde a última", 0), ("Desde a última tag", -1)];

enum Mensagem {
    Lista {
        workspace: i64,
        resultado: Result<(Vec<Rotina>, bool), String>,
    },
    Rodadas {
        rotina: i64,
        resultado: Result<Vec<Rodada>, String>,
    },
    Texto {
        rodada: i64,
        resultado: Result<Rodada, String>,
    },
    /// Uma mudança pedida ao núcleo terminou; o evento traz a lista nova.
    Feito(Result<(), String>),
}

/// A rotina sendo escrita: nova (`id` 0) ou uma existente.
struct Edicao {
    id: i64,
    nome: String,
    pedido: String,
    do_quadro: bool,
    do_codigo: bool,
    periodo: usize,
    dias: [bool; 7],
    hora: String,
    agendada: bool,
    ativa: bool,
}

impl Edicao {
    fn nova() -> Self {
        Edicao {
            id: 0,
            nome: String::new(),
            pedido: String::new(),
            do_quadro: true,
            do_codigo: false,
            periodo: 0,
            dias: [false; 7],
            hora: "17:00".into(),
            agendada: false,
            ativa: true,
        }
    }

    fn de(r: &Rotina) -> Self {
        let mut dias = [false; 7];
        for d in &r.agenda_dias {
            if (1..=7).contains(d) {
                dias[*d as usize - 1] = true;
            }
        }
        Edicao {
            id: r.id,
            nome: r.nome.clone(),
            pedido: r.pedido.clone(),
            do_quadro: r.do_quadro,
            do_codigo: r.do_codigo,
            periodo: PERIODOS.iter().position(|(_, d)| *d == r.dias).unwrap_or(0),
            dias,
            hora: if r.agenda_hora.is_empty() { "17:00".into() } else { r.agenda_hora.clone() },
            agendada: !r.agenda_dias.is_empty(),
            ativa: r.ativa,
        }
    }

    fn pedido(&self) -> NovaRotina {
        let agenda_dias = if self.agendada { self.dias.iter().enumerate().filter(|(_, d)| **d).map(|(i, _)| i as u8 + 1).collect() } else { Vec::new() };
        NovaRotina {
            nome: self.nome.trim().to_string(),
            pedido: self.pedido.trim().to_string(),
            do_quadro: self.do_quadro,
            do_codigo: self.do_codigo,
            dias: PERIODOS[self.periodo].1,
            agenda_dias,
            agenda_hora: self.hora.trim().to_string(),
            ativa: self.ativa,
        }
    }

    /// O que falta para poder salvar.
    fn pendente(&self) -> Option<&'static str> {
        if self.nome.trim().is_empty() {
            Some("Dê um nome à rotina.")
        } else if self.pedido.trim().is_empty() {
            Some("Escreva o que a rotina deve produzir.")
        } else if !self.do_quadro && !self.do_codigo {
            Some("Escolha pelo menos uma fonte.")
        } else if self.agendada && !self.dias.iter().any(|d| *d) {
            Some("Escolha os dias da agenda.")
        } else if self.agendada && !hora_valida(&self.hora) {
            Some("A hora é no formato 17:00.")
        } else {
            None
        }
    }
}

fn hora_valida(h: &str) -> bool {
    let h = h.trim();
    let Some((hh, mm)) = h.split_once(':') else { return false };
    matches!((hh.parse::<u8>(), mm.parse::<u8>()), (Ok(hh), Ok(mm)) if hh < 24 && mm < 60 && hh.to_string().len() <= 2 && mm.to_string().len() <= 2)
        && h.len() == 5
}

/// O que a tela precisa da janela.
pub struct Contexto<'a> {
    pub nome: &'a str,
    pub versao: u64,
    pub pode: bool,
}

pub struct TelaRotinas {
    workspace: i64,
    rotinas: Option<Result<(Vec<Rotina>, bool), String>>,
    versao_lida: Option<u64>,
    escolhida: Option<i64>,
    rodadas: Vec<Rodada>,
    /// O texto da rodada aberta na gaveta.
    texto: Option<(i64, Result<Rodada, String>)>,
    /// Qual rodada a gaveta mostra (None: a última).
    rodada_aberta: Option<i64>,
    listando_rodadas: bool,
    edicao: Option<Edicao>,
    remover: Option<i64>,
    erro: Option<String>,
    ocupado: bool,
    /// A lista falhou (o núcleo demorou, por exemplo): quantas vezes já
    /// tentou de novo sozinha e quando vai tentar outra vez.
    tentativas: u8,
    tentar_em: Option<Instant>,
    /// Uma leitura nova da lista está a caminho depois de uma falha.
    relendo: bool,
    envio: Sender<Mensagem>,
    chegando: Receiver<Mensagem>,
}

impl Default for TelaRotinas {
    fn default() -> Self {
        let (envio, chegando) = mpsc::channel();
        TelaRotinas {
            workspace: 0,
            rotinas: None,
            versao_lida: None,
            escolhida: None,
            rodadas: Vec::new(),
            texto: None,
            rodada_aberta: None,
            listando_rodadas: false,
            edicao: None,
            remover: None,
            erro: None,
            ocupado: false,
            tentativas: 0,
            tentar_em: None,
            relendo: false,
            envio,
            chegando,
        }
    }
}

impl TelaRotinas {
    pub fn abrir(&mut self, workspace: i64) {
        if self.workspace != workspace {
            self.workspace = workspace;
            self.rotinas = None;
            self.versao_lida = None;
            self.escolhida = None;
            self.rodadas.clear();
            self.texto = None;
            self.rodada_aberta = None;
            self.edicao = None;
            self.remover = None;
            self.erro = None;
            self.tentativas = 0;
            self.tentar_em = None;
            self.relendo = false;
        }
    }

    fn buscar_lista(&mut self, ctx: &egui::Context) {
        self.relendo = matches!(self.rotinas, Some(Err(_)));
        let workspace = self.workspace;
        registro::em_segundo_plano(&self.envio, ctx, move || Mensagem::Lista { workspace, resultado: api::rotinas(workspace) });
    }

    fn receber(&mut self) {
        while let Ok(m) = self.chegando.try_recv() {
            match m {
                Mensagem::Lista { workspace, resultado } if workspace == self.workspace => {
                    if let Ok((lista, _)) = &resultado
                        && self.escolhida.is_some_and(|id| !lista.iter().any(|r| r.id == id))
                    {
                        self.escolhida = None;
                        self.edicao = None;
                    }
                    // Falhou: tenta de novo sozinha, até três vezes, com um
                    // intervalo que cresce (2 s, 4 s, 6 s).
                    self.relendo = false;
                    match &resultado {
                        Ok(_) => self.tentativas = 0,
                        Err(_) if self.tentativas < 3 => {
                            self.tentativas += 1;
                            self.tentar_em = Some(Instant::now() + Duration::from_secs(2 * u64::from(self.tentativas)));
                        }
                        Err(_) => {}
                    }
                    self.rotinas = Some(resultado);
                }
                Mensagem::Lista { .. } => {}
                Mensagem::Rodadas { rotina, resultado } if Some(rotina) == self.escolhida => {
                    self.listando_rodadas = false;
                    if let Ok(r) = resultado {
                        self.rodadas = r;
                    }
                }
                Mensagem::Rodadas { .. } => {}
                Mensagem::Texto { rodada, resultado } => {
                    if self.texto.as_ref().is_none_or(|(id, _)| *id == rodada) || self.rodada_aberta == Some(rodada) {
                        self.texto = Some((rodada, resultado));
                    }
                }
                Mensagem::Feito(r) => {
                    self.ocupado = false;
                    match r {
                        Ok(()) => {
                            self.erro = None;
                            self.edicao = None;
                            self.remover = None;
                        }
                        Err(e) => self.erro = Some(e),
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

    fn buscar_rodadas(&mut self, ctx: &egui::Context, rotina: i64) {
        self.listando_rodadas = true;
        registro::em_segundo_plano(&self.envio, ctx, move || Mensagem::Rodadas { rotina, resultado: api::rodadas(rotina) });
    }

    fn buscar_texto(&mut self, ctx: &egui::Context, rodada: i64) {
        if self.texto.as_ref().is_some_and(|(id, r)| *id == rodada && r.is_ok()) {
            return;
        }
        self.texto = Some((rodada, Err(String::new())));
        registro::em_segundo_plano(&self.envio, ctx, move || Mensagem::Texto { rodada, resultado: api::rodada(rodada) });
    }

    fn escolher(&mut self, ctx: &egui::Context, rotina: &Rotina) {
        if self.escolhida != Some(rotina.id) {
            self.escolhida = Some(rotina.id);
            self.rodadas.clear();
            self.rodada_aberta = None;
            self.texto = None;
            self.edicao = None;
            self.buscar_rodadas(ctx, rotina.id);
        }
        if let Some(u) = &rotina.ultima
            && u.estado == "ok"
        {
            self.buscar_texto(ctx, u.id);
        }
    }

    pub fn mostrar(&mut self, ui: &mut egui::Ui, cx: &Contexto) {
        self.receber();
        if self.versao_lida != Some(cx.versao) {
            self.versao_lida = Some(cx.versao);
            self.buscar_lista(ui.ctx());
            if let Some(id) = self.escolhida {
                self.buscar_rodadas(ui.ctx(), id);
            }
        }
        if let Some(t) = self.tentar_em {
            let agora = Instant::now();
            if agora >= t {
                self.tentar_em = None;
                self.buscar_lista(ui.ctx());
            } else {
                ui.ctx().request_repaint_after(t - agora);
            }
        }
        // A última rodada da escolhida mudou (terminou agora): o texto vem de novo.
        if let (Some(id), Some(Ok((lista, _)))) = (self.escolhida, &self.rotinas)
            && self.rodada_aberta.is_none()
            && let Some(u) = lista.iter().find(|r| r.id == id).and_then(|r| r.ultima.as_ref())
            && u.estado == "ok"
            && self.texto.as_ref().is_none_or(|(t, _)| *t != u.id)
        {
            let rodada = u.id;
            self.buscar_texto(ui.ctx(), rodada);
        }
        let p = cores();
        let com_gaveta = self.escolhida.is_some() || self.edicao.as_ref().is_some_and(|e| e.id == 0);
        let total = ui.available_rect_before_wrap();
        let largura_lista = if com_gaveta { (total.width() - LARGURA_GAVETA - 16.0).max(360.0) } else { total.width() };
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(vec2(largura_lista, total.height()), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width(largura_lista);
                egui::ScrollArea::vertical().id_salt("rotinas-lista").auto_shrink([false, false]).show(ui, |ui| {
                    ui.add_space(8.0);
                    self.lista(ui, cx);
                    ui.add_space(24.0);
                });
            });
            if com_gaveta {
                ui.add_space(16.0);
                ui.allocate_ui_with_layout(vec2(LARGURA_GAVETA, total.height()), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_width(LARGURA_GAVETA);
                    ui.add_space(8.0);
                    egui::Frame::new().fill(p.superficie).stroke(Stroke::new(1.0, p.borda)).corner_radius(CornerRadius::same(12)).show(ui, |ui| {
                        ui.set_width(LARGURA_GAVETA);
                        ui.set_min_height(total.height() - 24.0);
                        self.gaveta(ui, cx);
                    });
                });
            }
        });
    }

    fn lista(&mut self, ui: &mut egui::Ui, cx: &Contexto) {
        let p = cores();
        let (rotinas, claude) = match &self.rotinas {
            None => {
                tema::cabecalho(ui, "Rotinas", "");
                for _ in 0..3 {
                    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::hover());
                    tema::esqueleto(ui.painter(), r, 10);
                    ui.add_space(6.0);
                }
                return;
            }
            Some(Err(e)) => {
                let erro = e.clone();
                tema::cabecalho(ui, "Rotinas", "");
                ui.label(RichText::new(format!("Não consegui ler as rotinas: {erro}")).color(p.erro));
                ui.add_space(6.0);
                if self.tentar_em.is_some() || self.relendo {
                    ui.label(RichText::new("Tentando de novo…").color(p.suave).size(12.5));
                } else if tema::botao_contorno(ui, "Tentar de novo", p.texto).clicked() {
                    self.tentativas = 0;
                    self.buscar_lista(ui.ctx());
                }
                return;
            }
            Some(Ok((r, c))) => (r.clone(), *c),
        };
        let agendadas = rotinas.iter().filter(|r| !r.agenda_dias.is_empty() && r.ativa).count();
        let proxima = rotinas.iter().filter(|r| !r.proxima_texto.is_empty()).map(|r| r.proxima_texto.as_str()).next();
        let explicacao = match (rotinas.len(), agendadas, proxima) {
            (0, ..) => format!(
                "Pedidos que rodam sozinhos no horário ou quando você manda, a partir do quadro de {} e dos commits dos projetos. Cada rodada vira um texto para copiar ou ler na reunião.",
                cx.nome
            ),
            (n, 0, _) => format!("{} · nenhuma agendada", plural(n, "rotina", "rotinas")),
            (n, a, Some(prox)) => format!("{} · {} · a próxima roda {prox}", plural(n, "rotina", "rotinas"), plural(a, "agendada", "agendadas")),
            (n, a, None) => format!("{} · {}", plural(n, "rotina", "rotinas"), plural(a, "agendada", "agendadas")),
        };
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_width(ui.available_width() - 150.0);
                tema::cabecalho(ui, "Rotinas", &explicacao);
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                ui.add_space(4.0);
                if cx.pode && self.edicao.is_none() && tema::botao_principal(ui, "+ Nova rotina", true).clicked() {
                    self.escolhida = None;
                    self.rodadas.clear();
                    self.texto = None;
                    self.edicao = Some(Edicao::nova());
                }
            });
        });
        ui.add_space(14.0);
        if let Some(e) = &self.erro
            && self.edicao.is_none()
        {
            ui.label(RichText::new(e).color(p.erro).size(13.0));
            ui.add_space(8.0);
        }
        if !claude {
            ui.label(RichText::new("As rotinas precisam do Claude Code instalado, e não o achei neste computador.").color(p.alerta).size(13.0));
            ui.add_space(8.0);
        }
        if rotinas.is_empty() {
            self.vazio(ui, cx);
            return;
        }
        egui::Frame::new()
            .fill(p.superficie)
            .stroke(Stroke::new(1.0, p.borda))
            .corner_radius(CornerRadius::same(12))
            .inner_margin(egui::Margin { left: 6, right: 6, top: 4, bottom: 6 })
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let largura = ui.available_width();
                let estreita = largura < 760.0;
                self.cabecalho_da_tabela(ui, estreita);
                for r in &rotinas {
                    self.linha(ui, r, cx, estreita);
                }
            });
        ui.add_space(10.0);
        ui.label(
            RichText::new(
                "As rotinas rodam com o Claude Code do perfil, sem ferramentas, lendo o quadro e os commits. A agenda só dispara com a Colmeia aberta.",
            )
            .color(p.suave)
            .size(12.5),
        );
    }

    fn cabecalho_da_tabela(&self, ui: &mut egui::Ui, estreita: bool) {
        let p = cores();
        let fonte = egui::FontId::proportional(11.0);
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::hover());
        let pintor = ui.painter();
        let mut x = rect.left() + 36.0;
        let colunas = self.colunas(rect.width(), estreita);
        for (titulo, largura) in colunas {
            pintor.text(egui::pos2(x, rect.center().y), egui::Align2::LEFT_CENTER, titulo, fonte.clone(), p.suave);
            x += largura;
        }
        pintor.hline(rect.left() + 8.0..=rect.right() - 8.0, rect.bottom(), Stroke::new(1.0, p.borda));
    }

    /// As colunas da tabela: nome (o que sobrar), agenda, última, próxima.
    /// Com a gaveta aberta (estreita), ficam só nome e última; a agenda vai
    /// para a linha do nome.
    fn colunas(&self, largura: f32, estreita: bool) -> Vec<(&'static str, f32)> {
        let acoes = 76.0;
        if estreita {
            let fixa = 118.0;
            return vec![("ROTINA", (largura - 36.0 - fixa - acoes).max(120.0)), ("ÚLTIMA RODADA", fixa)];
        }
        let fixa = 150.0;
        vec![("ROTINA", (largura - 36.0 - fixa * 3.0 - acoes).max(160.0)), ("AGENDA", fixa), ("ÚLTIMA RODADA", fixa), ("PRÓXIMA", fixa)]
    }

    fn linha(&mut self, ui: &mut egui::Ui, r: &Rotina, cx: &Contexto, estreita: bool) {
        let p = cores();
        let ativa = self.escolhida == Some(r.id);
        let (rect, resposta) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::click());
        let resposta = resposta.on_hover_cursor(egui::CursorIcon::PointingHand);
        let fundo = if ativa {
            p.realce
        } else if resposta.hovered() {
            tema::misturar(p.superficie, p.realce, 0.5)
        } else {
            p.superficie
        };
        let pintor = ui.painter();
        pintor.rect_filled(rect, CornerRadius::same(8), fundo);
        pintor.hline(rect.left() + 8.0..=rect.right() - 8.0, rect.bottom(), Stroke::new(1.0, p.borda.gamma_multiply(0.6)));
        let (cor_ponto, dica) = match &r.ultima {
            Some(u) if u.estado == "erro" => (p.erro, "A última rodada falhou"),
            Some(u) if u.estado == "rodando" => (p.alerta, "Rodando agora"),
            _ if !r.ativa => (p.suave, "Rotina pausada"),
            _ => (p.ok, "Tudo certo"),
        };
        pintor.circle_filled(egui::pos2(rect.left() + 20.0, rect.center().y), 4.0, cor_ponto);
        let colunas = self.colunas(rect.width(), estreita);
        let mut x = rect.left() + 36.0;
        let fonte = egui::FontId::proportional(13.0);
        let fontes = match (r.do_quadro, r.do_codigo) {
            (true, true) => "quadro e commits".to_string(),
            (false, true) => "commits".to_string(),
            _ => "quadro".to_string(),
        };
        let fontes = if estreita { agenda_curta(r) } else { fontes };
        let nome = pintor.layout_no_wrap(r.nome.clone(), tema::forte(13.0), p.texto);
        let largura_nome = colunas[0].1 - 16.0;
        let mut pos = egui::pos2(x, rect.center().y - nome.size().y / 2.0);
        if nome.size().x > largura_nome {
            tema::texto_com_fim(pintor, pos, &r.nome, "", tema::forte(13.0), p.texto, p.texto, largura_nome);
        } else {
            pintor.galley(pos, nome.clone(), p.texto);
            pos.x += nome.size().x + 6.0;
            let resto = (largura_nome - nome.size().x - 6.0).max(0.0);
            if resto > 40.0 {
                tema::texto_com_fim(pintor, egui::pos2(pos.x, rect.center().y - 8.0), &format!("· {fontes}"), "", fonte.clone(), p.suave, p.suave, resto);
            }
        }
        x += colunas[0].1;
        if !estreita {
            let agenda = agenda_curta(r);
            tema::texto_com_fim(pintor, egui::pos2(x, rect.center().y - 8.0), &agenda, "", fonte.clone(), p.suave, p.suave, colunas[1].1 - 12.0);
            x += colunas[1].1;
        }
        let (ultima, cor_ultima) = match &r.ultima {
            None => ("nunca rodou".to_string(), p.suave),
            Some(u) if u.estado == "erro" => (format!("Falhou {}", u.momento_texto), p.erro),
            Some(u) if u.estado == "rodando" => ("rodando…".to_string(), p.alerta),
            Some(u) => (u.momento_texto.clone(), p.suave),
        };
        let largura_ultima = colunas[if estreita { 1 } else { 2 }].1;
        tema::texto_com_fim(pintor, egui::pos2(x, rect.center().y - 8.0), &ultima, "", fonte.clone(), cor_ultima, cor_ultima, largura_ultima - 12.0);
        x += largura_ultima;
        if !estreita {
            let proxima = if r.proxima_texto.is_empty() { "—".to_string() } else { r.proxima_texto.clone() };
            tema::texto_com_fim(pintor, egui::pos2(x, rect.center().y - 8.0), &proxima, "", fonte, p.suave, p.suave, colunas[3].1 - 12.0);
        }
        // As ações, por cima da linha.
        let acoes = egui::Rect::from_min_max(egui::pos2(rect.right() - 76.0, rect.top()), rect.max);
        let mut sub = ui.new_child(egui::UiBuilder::new().max_rect(acoes).layout(egui::Layout::right_to_left(egui::Align::Center)));
        let rodando = r.ultima.as_ref().is_some_and(|u| u.estado == "rodando");
        let id = r.id;
        let mut clicou_acao = false;
        sub.add_space(8.0);
        let mais = tema::botao_icone(&mut sub, Icone::Painel, 28.0).on_hover_text("Editar ou apagar");
        if mais.clicked() {
            clicou_acao = true;
            self.escolher(ui.ctx(), r);
            self.edicao = Some(Edicao::de(r));
        }
        let rodar = tema::botao_icone_com(&mut sub, Icone::Agente, 28.0, cx.pode && !rodando && !self.ocupado).on_hover_text(if rodando {
            "Rodando agora"
        } else {
            "Rodar agora"
        });
        if rodar.clicked() {
            clicou_acao = true;
            self.escolher(ui.ctx(), r);
            self.pedir(ui.ctx(), move || api::rodar_rotina(id).map(|_| ()));
        }
        if resposta.clicked() && !clicou_acao {
            self.escolher(ui.ctx(), r);
        }
        resposta.on_hover_text(dica);
    }

    fn vazio(&mut self, ui: &mut egui::Ui, cx: &Contexto) {
        let p = cores();
        egui::Frame::new().fill(p.superficie).corner_radius(CornerRadius::same(14)).inner_margin(egui::Margin::same(22)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical_centered(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(44.0, 44.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 22.0, tema::misturar(p.superficie, p.destaque, 0.15));
                tema::desenhar_icone(ui.painter(), r.center(), Icone::Relogio, p.destaque);
                ui.add_space(10.0);
                ui.label(tema::texto_forte("Nenhuma rotina ainda", 15.5).color(p.texto));
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Uma rotina é um pedido que você escreve uma vez, como \"liste o que entrou na semana, por projeto, em Corrigido, Melhorado e Novo\", e roda sempre que você mandar ou no horário que escolher.")
                        .color(p.suave)
                        .size(13.0),
                );
                ui.add_space(14.0);
                if cx.pode && tema::botao_principal(ui, "Criar a primeira rotina", true).clicked() {
                    self.edicao = Some(Edicao::nova());
                }
            });
        });
    }

    fn gaveta(&mut self, ui: &mut egui::Ui, cx: &Contexto) {
        if self.edicao.is_some() {
            self.editor(ui, cx);
            return;
        }
        let p = cores();
        let Some(id) = self.escolhida else { return };
        let Some(rotina) = self.rotinas.as_ref().and_then(|r| r.as_ref().ok()).and_then(|(l, _)| l.iter().find(|r| r.id == id)).cloned() else { return };
        let aberta = match self.rodada_aberta {
            Some(rid) => self.rodadas.iter().find(|r| r.id == rid).cloned(),
            None => rotina.ultima.clone(),
        };
        // Cabeçalho: nome, quando rodou, copiar e fechar.
        egui::Frame::new().inner_margin(egui::Margin { left: 16, right: 10, top: 12, bottom: 10 }).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(ui.available_width() - 100.0);
                    ui.label(tema::texto_forte(&rotina.nome, 14.0).color(p.texto));
                    let quando = match &aberta {
                        None => "ainda não rodou".to_string(),
                        Some(r) if r.estado == "rodando" => format!("rodando desde {}", r.momento_texto),
                        Some(r) => format!("{} · {}", r.momento_texto, if r.origem == "agenda" { "pela agenda" } else { "você mandou" }),
                    };
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(quando).color(p.suave).size(11.5));
                        let n = self.rodadas.len();
                        if n > 1 {
                            let texto = if self.rodada_aberta.is_some() { "voltar à última".to_string() } else { format!("ver as {n} rodadas") };
                            if tema::link_com(ui, &texto, true).clicked() {
                                if self.rodada_aberta.is_some() {
                                    self.rodada_aberta = None;
                                    if let Some(u) = &rotina.ultima {
                                        let rid = u.id;
                                        self.buscar_texto(ui.ctx(), rid);
                                    }
                                } else {
                                    self.rodada_aberta = Some(-1);
                                }
                            }
                        }
                    });
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if tema::botao_icone(ui, Icone::Fechar, 26.0).on_hover_text("Fechar").clicked() {
                        self.escolhida = None;
                        self.rodadas.clear();
                        self.texto = None;
                        self.rodada_aberta = None;
                    }
                    let texto = self.texto.as_ref().and_then(|(_, r)| r.as_ref().ok()).map(|r| r.texto.clone()).filter(|t| !t.is_empty());
                    if let Some(t) = texto
                        && tema::botao_icone(ui, Icone::Duplicar, 26.0).on_hover_text("Copiar o texto").clicked()
                    {
                        ui.ctx().copy_text(t);
                    }
                });
            });
        });
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
        ui.painter().hline(r.x_range(), r.center().y, Stroke::new(1.0, p.borda));
        // O corpo: a lista de rodadas, ou o texto de uma.
        let altura_rodape = 52.0;
        let altura = (ui.available_height() - altura_rodape).max(120.0);
        egui::ScrollArea::vertical().id_salt(("rotina-gaveta", id)).max_height(altura).auto_shrink([false, false]).show(ui, |ui| {
            egui::Frame::new().inner_margin(egui::Margin { left: 18, right: 18, top: 14, bottom: 14 }).show(ui, |ui| {
                ui.set_width(ui.available_width());
                if self.rodada_aberta == Some(-1) {
                    self.lista_de_rodadas(ui);
                } else {
                    self.corpo_da_rodada(ui, &rotina, aberta.as_ref());
                }
            });
        });
        // Rodapé: editar e rodar de novo.
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            egui::Frame::new().inner_margin(egui::Margin { left: 14, right: 14, top: 10, bottom: 10 }).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    if cx.pode && tema::botao_secundario(ui, "Editar rotina").clicked() {
                        self.edicao = Some(Edicao::de(&rotina));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let rodando = rotina.ultima.as_ref().is_some_and(|u| u.estado == "rodando");
                        let texto = if rotina.ultima.is_some() { "Rodar de novo" } else { "Rodar agora" };
                        if tema::botao_principal(ui, texto, cx.pode && !rodando && !self.ocupado).clicked() {
                            self.rodada_aberta = None;
                            self.pedir(ui.ctx(), move || api::rodar_rotina(id).map(|_| ()));
                        }
                    });
                });
            });
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
            ui.painter().hline(r.x_range(), r.center().y, Stroke::new(1.0, p.borda));
        });
    }

    fn lista_de_rodadas(&mut self, ui: &mut egui::Ui) {
        let p = cores();
        ui.label(RichText::new("RODADAS").color(p.suave).size(11.0).strong());
        ui.add_space(6.0);
        let rodadas = self.rodadas.clone();
        for r in &rodadas {
            let (rect, resposta) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::click());
            let resposta = resposta.on_hover_cursor(egui::CursorIcon::PointingHand);
            if resposta.hovered() {
                ui.painter().rect_filled(rect, CornerRadius::same(8), p.realce);
            }
            let cor = match r.estado.as_str() {
                "erro" => p.erro,
                "rodando" => p.alerta,
                _ => p.ok,
            };
            ui.painter().circle_filled(egui::pos2(rect.left() + 12.0, rect.center().y), 4.0, cor);
            ui.painter().text(
                egui::pos2(rect.left() + 26.0, rect.top() + 8.0),
                egui::Align2::LEFT_TOP,
                &r.momento_texto,
                egui::FontId::proportional(13.0),
                p.texto,
            );
            let detalhe = match r.estado.as_str() {
                "erro" => r.erro.clone(),
                "rodando" => "rodando…".to_string(),
                _ => format!("{} · {}", r.resumo, if r.origem == "agenda" { "pela agenda" } else { "você mandou" }),
            };
            tema::texto_com_fim(
                ui.painter(),
                egui::pos2(rect.left() + 26.0, rect.top() + 25.0),
                &detalhe,
                "",
                egui::FontId::proportional(11.5),
                if r.estado == "erro" { p.erro } else { p.suave },
                p.suave,
                rect.width() - 30.0,
            );
            if resposta.clicked() && r.estado == "ok" {
                self.rodada_aberta = Some(r.id);
                let rid = r.id;
                self.buscar_texto(ui.ctx(), rid);
            }
        }
        if rodadas.is_empty() {
            ui.label(RichText::new("Nenhuma rodada ainda.").color(p.suave).size(13.0));
        }
    }

    fn corpo_da_rodada(&mut self, ui: &mut egui::Ui, rotina: &Rotina, aberta: Option<&Rodada>) {
        let p = cores();
        match aberta {
            None => {
                ui.label(RichText::new(&rotina.pedido).color(p.texto).size(13.0));
                ui.add_space(10.0);
                ui.label(
                    RichText::new("Esta rotina ainda não rodou. \"Rodar agora\" pede o texto ao Claude Code com o quadro do período.")
                        .color(p.suave)
                        .size(12.5),
                );
            }
            Some(r) if r.estado == "rodando" => {
                ui.label(RichText::new("O Claude Code está escrevendo…").color(p.alerta).size(13.0));
                ui.add_space(8.0);
                for _ in 0..4 {
                    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 16.0), Sense::hover());
                    tema::esqueleto(ui.painter(), rect, 6);
                    ui.add_space(6.0);
                }
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(600));
            }
            Some(r) if r.estado == "erro" => {
                ui.label(RichText::new(format!("A rodada falhou: {}", r.erro)).color(p.erro).size(13.0));
                ui.add_space(8.0);
                ui.label(RichText::new(format!("Período {} a {}.", data_curta(&r.de), data_curta(&r.ate))).color(p.suave).size(12.5));
            }
            Some(r) => match &self.texto {
                Some((id, Ok(t))) if *id == r.id => texto_markdown(ui, &t.texto),
                Some((id, Err(e))) if *id == r.id && !e.is_empty() => {
                    ui.label(RichText::new(format!("Não consegui ler o texto: {e}")).color(p.erro).size(13.0));
                }
                _ => {
                    for _ in 0..6 {
                        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 16.0), Sense::hover());
                        tema::esqueleto(ui.painter(), rect, 6);
                        ui.add_space(6.0);
                    }
                }
            },
        }
    }

    fn editor(&mut self, ui: &mut egui::Ui, cx: &Contexto) {
        let p = cores();
        let Some(mut e) = self.edicao.take() else { return };
        let mut salvar = false;
        let mut cancelar = false;
        let mut apagar = false;
        egui::ScrollArea::vertical().id_salt("rotina-editor").auto_shrink([false, false]).show(ui, |ui| {
            egui::Frame::new().inner_margin(egui::Margin { left: 16, right: 16, top: 14, bottom: 14 }).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(tema::texto_forte(if e.id == 0 { "Nova rotina" } else { "Editar rotina" }, 15.0).color(p.texto));
                ui.add_space(10.0);
                tema::campo(ui, "Nome", &mut e.nome, "Changelog da semana");
                if e.nome.chars().count() > MAX_NOME {
                    e.nome = e.nome.chars().take(MAX_NOME).collect();
                }
                ui.add_space(10.0);
                ui.label(RichText::new("O pedido").color(p.suave).size(11.5).strong());
                ui.add_space(4.0);
                tema::campo_multilinha(ui, &mut e.pedido, 5, 180.0, egui::Id::new(("rotina-pedido", e.id)), false);
                if e.pedido.chars().count() > MAX_PEDIDO {
                    e.pedido = e.pedido.chars().take(MAX_PEDIDO).collect();
                }
                ui.label(RichText::new("Diga o que o texto é, para quem é e como deve soar. Exemplo: \"Liste o que entrou na semana, por projeto, em Corrigido, Melhorado e Novo, na linguagem de quem usa a loja.\"").color(p.suave).size(11.5));
                ui.add_space(12.0);
                ui.label(RichText::new("De onde vem").color(p.suave).size(11.5).strong());
                ui.add_space(4.0);
                tema::caixa_marcar(ui, "Quadro: tarefas, frases da daily e da sprint, notas", &mut e.do_quadro);
                tema::caixa_marcar(ui, "Commits dos projetos no período", &mut e.do_codigo);
                ui.add_space(12.0);
                ui.label(RichText::new("Período").color(p.suave).size(11.5).strong());
                ui.add_space(4.0);
                let nomes: Vec<&str> = PERIODOS.iter().map(|(n, _)| *n).collect();
                if let Some(i) = tema::segmentado(ui, &nomes, e.periodo) {
                    e.periodo = i;
                }
                let dica = match PERIODOS[e.periodo].1 {
                    0 => "Desde a última rodada que deu certo; na primeira, 7 dias.",
                    -1 => "Os commits de cada projeto desde a última tag dele (a versão lançada); o quadro desde a tag mais antiga. Sem tag, 30 dias.",
                    _ => "",
                };
                if !dica.is_empty() {
                    ui.add_space(4.0);
                    ui.add(egui::Label::new(RichText::new(dica).color(p.suave).size(12.0)).wrap());
                }
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Agenda").color(p.suave).size(11.5).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if tema::chip_alternar(ui, if e.agendada { "Ligada" } else { "Sob demanda" }, e.agendada).clicked() {
                            e.agendada = !e.agendada;
                        }
                    });
                });
                if e.agendada {
                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        for (i, nome) in DIAS.iter().enumerate() {
                            if tema::chip_alternar(ui, nome, e.dias[i]).clicked() {
                                e.dias[i] = !e.dias[i];
                            }
                        }
                    });
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("às").color(p.suave).size(13.0));
                        ui.add(egui::TextEdit::singleline(&mut e.hora).desired_width(64.0).hint_text("17:00"));
                        ui.label(RichText::new("no horário deste computador, com a Colmeia aberta").color(p.suave).size(12.0));
                    });
                }
                ui.add_space(12.0);
                tema::caixa_marcar(ui, "Rotina ativa", &mut e.ativa);
                if let Some(erro) = &self.erro {
                    ui.add_space(8.0);
                    ui.label(RichText::new(erro).color(p.erro).size(12.5));
                }
                ui.add_space(16.0);
                let pendente = e.pendente();
                ui.horizontal(|ui| {
                    if tema::botao_principal(ui, "Salvar", cx.pode && pendente.is_none() && !self.ocupado).clicked() {
                        salvar = true;
                    }
                    if tema::botao_secundario(ui, "Cancelar").clicked() {
                        cancelar = true;
                    }
                    if e.id != 0 {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if self.remover == Some(e.id) {
                                if tema::botao_alerta(ui, "Apagar mesmo").clicked() {
                                    apagar = true;
                                }
                                if tema::botao_secundario(ui, "Não").clicked() {
                                    self.remover = None;
                                }
                            } else if tema::botao_contorno(ui, "Apagar…", p.erro).clicked() {
                                self.remover = Some(e.id);
                            }
                        });
                    }
                });
                if let Some(falta) = pendente {
                    ui.add_space(6.0);
                    ui.label(RichText::new(falta).color(p.suave).size(12.0));
                }
            });
        });
        if cancelar {
            self.erro = None;
            self.remover = None;
            return;
        }
        if apagar {
            let id = e.id;
            self.escolhida = None;
            self.pedir(ui.ctx(), move || api::remover_rotina(id));
            return;
        }
        if salvar {
            let pedido = e.pedido();
            let (id, workspace) = (e.id, self.workspace);
            self.edicao = Some(e);
            self.pedir(ui.ctx(), move || if id == 0 { api::criar_rotina(workspace, &pedido).map(|_| ()) } else { api::editar_rotina(id, &pedido).map(|_| ()) });
            return;
        }
        self.edicao = Some(e);
    }
}

fn plural(n: usize, um: &str, varios: &str) -> String {
    if n == 1 { format!("1 {um}") } else { format!("{n} {varios}") }
}

/// "Sexta às 17:00", "Dias úteis às 08:30", "Sob demanda".
fn agenda_curta(r: &Rotina) -> String {
    if r.agenda_dias.is_empty() {
        return "Sob demanda".to_string();
    }
    if !r.ativa {
        return "Pausada".to_string();
    }
    let mut dias: Vec<u8> = r.agenda_dias.clone();
    dias.sort_unstable();
    let nomes = ["Segunda", "Terça", "Quarta", "Quinta", "Sexta", "Sábado", "Domingo"];
    let quando = match dias.as_slice() {
        [1, 2, 3, 4, 5] => "Dias úteis".to_string(),
        [1, 2, 3, 4, 5, 6, 7] => "Todo dia".to_string(),
        [d] => nomes[(*d as usize - 1).min(6)].to_string(),
        ds => ds.iter().map(|d| &DIAS[(*d as usize - 1).min(6)][..3]).collect::<Vec<_>>().join(", "),
    };
    format!("{quando} às {}", r.agenda_hora)
}

/// "2026-10-14" vira "14/10".
fn data_curta(d: &str) -> String {
    match d.split('-').collect::<Vec<_>>().as_slice() {
        [_, m, dia] => format!("{dia}/{m}"),
        _ => d.to_string(),
    }
}

/// O texto da rodada, em Markdown simples: títulos, listas e parágrafos.
fn texto_markdown(ui: &mut egui::Ui, texto: &str) {
    let p = cores();
    let mut vazio = false;
    for linha in texto.lines() {
        let l = linha.trim_end();
        if l.trim().is_empty() {
            if !vazio {
                ui.add_space(6.0);
            }
            vazio = true;
            continue;
        }
        vazio = false;
        let sem_cerquilha = l.trim_start_matches('#');
        if sem_cerquilha.len() != l.len() {
            let nivel = l.len() - sem_cerquilha.len();
            ui.add_space(if nivel <= 3 { 8.0 } else { 4.0 });
            ui.label(tema::texto_forte(sem_cerquilha.trim(), if nivel <= 3 { 15.0 } else { 13.5 }).color(if nivel <= 3 { p.texto } else { p.destaque }));
            ui.add_space(2.0);
        } else if l.trim().starts_with("**") && l.trim().ends_with("**") && l.trim().len() > 4 {
            ui.add_space(4.0);
            ui.label(tema::texto_forte(l.trim().trim_matches('*'), 13.5).color(p.destaque));
        } else if let Some(item) = l.trim_start().strip_prefix("- ").or_else(|| l.trim_start().strip_prefix("* ")) {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(RichText::new("•").color(p.suave).size(13.5));
                ui.label(RichText::new(sem_marcas(item)).color(p.texto).size(13.5));
            });
        } else {
            ui.label(RichText::new(sem_marcas(l.trim())).color(p.texto).size(13.5));
        }
    }
}

/// Tira os asteriscos de negrito e as crases de código (o texto fica limpo).
fn sem_marcas(texto: &str) -> String {
    texto.replace("**", "").replace('`', "")
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn agenda_curta_fala_como_a_tela() {
        let mut r = Rotina { agenda_dias: vec![5], agenda_hora: "17:00".into(), ativa: true, ..Default::default() };
        assert_eq!(agenda_curta(&r), "Sexta às 17:00");
        r.agenda_dias = vec![1, 2, 3, 4, 5];
        r.agenda_hora = "08:30".into();
        assert_eq!(agenda_curta(&r), "Dias úteis às 08:30");
        r.agenda_dias = vec![1, 3];
        assert_eq!(agenda_curta(&r), "Seg, Qua às 08:30");
        r.ativa = false;
        assert_eq!(agenda_curta(&r), "Pausada");
        r.agenda_dias.clear();
        assert_eq!(agenda_curta(&r), "Sob demanda");
    }

    #[test]
    fn edicao_vai_e_volta_da_rotina() {
        let r = Rotina {
            id: 3,
            nome: "Changelog".into(),
            pedido: "Liste.".into(),
            do_quadro: true,
            do_codigo: true,
            dias: 14,
            agenda_dias: vec![5],
            agenda_hora: "17:00".into(),
            ativa: true,
            ..Default::default()
        };
        let e = Edicao::de(&r);
        assert!(e.agendada && e.dias[4] && e.periodo == 1 && e.pendente().is_none());
        let n = e.pedido();
        assert_eq!((n.dias, n.agenda_dias, n.agenda_hora.as_str()), (14, vec![5], "17:00"));
        let mut nova = Edicao::nova();
        assert_eq!(nova.pendente(), Some("Dê um nome à rotina."));
        nova.nome = "X".into();
        nova.pedido = "Y".into();
        nova.agendada = true;
        assert_eq!(nova.pendente(), Some("Escolha os dias da agenda."));
        nova.dias[0] = true;
        nova.hora = "25:00".into();
        assert_eq!(nova.pendente(), Some("A hora é no formato 17:00."));
        nova.hora = "09:05".into();
        assert!(nova.pendente().is_none());
        assert!(nova.pedido().agenda_dias == vec![1]);
    }
}
