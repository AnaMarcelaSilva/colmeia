//! Linha do tempo do perfil (ou do projeto em foco) e o painel de daily e
//! sprint. Os textos vêm prontos do núcleo; aqui só se mostra, copia e salva.
//!
//! Desempenho: a lista é virtualizada com alturas fixas (só o que aparece é
//! desenhado), as páginas e as miniaturas são buscadas fora da thread da tela,
//! e as miniaturas ficam num cache limitado. Nada se atualiza por tempo: um
//! evento novo traz a primeira página de novo (no topo) ou mostra a pílula
//! "novidades" (rolado para baixo).

use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{self, Receiver, Sender};

use eframe::egui::text::{LayoutJob, TextWrapping};
use eframe::egui::{
    self, Color32, ColorImage, CornerRadius, FontId, Id, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, TextureHandle, TextureOptions, pos2, vec2,
};

use crate::api;
use crate::tema::{self, EstadoVisual, TipoAviso, cores, forte};

/// O que a linha do tempo pede à tela principal.
pub enum Acao {
    AbrirTarefa { tarefa: i64, agente: Option<i64> },
    IrParaQuadro,
    VerTodos,
    Avisar(TipoAviso, String),
}

#[derive(Clone, Copy, PartialEq)]
enum Aba {
    Daily,
    Sprint,
}

#[derive(Clone, Copy, PartialEq)]
enum Periodo {
    Dias7,
    Dias14,
    Mes,
    Escolher,
}

/// Perfil e projeto da tela: respostas de outro escopo são descartadas.
type Chave = (i64, Option<i64>);

enum Resposta {
    Pagina { chave: Chave, primeira: bool, resultado: Result<api::PaginaLinha, String> },
    Daily { chave: Chave, resultado: Result<api::Daily, String> },
    Sprint { chave: Chave, resultado: Result<api::Sprint, String> },
    Miniatura { id: i64, grande: bool, resultado: Result<ColorImage, String> },
    Salvo(Result<String, String>),
    Removido(Result<(), String>),
}

enum Miniatura {
    Carregando,
    Pronta(TextureHandle),
    Falhou,
}

/// Imagem aberta em tamanho grande.
struct Visor {
    anexo: i64,
    texto: String,
    quando: String,
    tarefa: i64,
    imagem: Option<Result<TextureHandle, String>>,
    confirmar: bool,
}

// Alturas fixas: a virtualização calcula a posição sem montar o texto.
const CABECALHO: f32 = 44.0;
const ITEM: f32 = 36.0;
const ITEM_COM_IMAGENS: f32 = 110.0;
const RESPIRO_BLOCO: f32 = 6.0;
const ENTRE_DIAS: f32 = 20.0;
const LINHA_ESPECIAL: f32 = 48.0;
const LARGURA_PAINEL: f32 = 420.0;
const MAX_MINIATURAS: usize = 64;
/// Altura das miniaturas guardadas (o dobro do que aparece, para telas de alta densidade).
const ALTURA_MINIATURA: u32 = 128;

pub struct Linha {
    chave: Option<Chave>,
    dias: Vec<api::Dia>,
    proximo: i64,
    criado_em: String,
    carregando: bool,
    carregou: bool,
    erro: Option<String>,
    canal: (Sender<Resposta>, Receiver<Resposta>),
    novidades: usize,
    no_topo: bool,
    ir_ao_topo: bool,
    pub painel_aberto: bool,
    painel_decidido: bool,
    aba: Aba,
    daily: Option<api::Daily>,
    texto_daily: String,
    focar_daily: bool,
    erro_daily: Option<String>,
    sprint: Option<api::Sprint>,
    texto_sprint: String,
    periodo: Periodo,
    de: String,
    ate: String,
    erro_sprint: Option<String>,
    miniaturas: HashMap<i64, Miniatura>,
    uso: VecDeque<i64>,
    visor: Option<Visor>,
    salvando: bool,
    /// Chegou evento com a linha do tempo fora da tela: ao voltar, a primeira
    /// página e os resumos são buscados de novo (sem consulta periódica).
    suja: bool,
    /// O núcleo está ligado; sem ele, nada é pedido nem copiado.
    pub conectado: bool,
}

impl Default for Linha {
    fn default() -> Self {
        Linha {
            chave: None,
            dias: Vec::new(),
            proximo: 0,
            criado_em: String::new(),
            carregando: false,
            carregou: false,
            erro: None,
            canal: mpsc::channel(),
            novidades: 0,
            no_topo: true,
            ir_ao_topo: false,
            painel_aberto: true,
            painel_decidido: false,
            aba: Aba::Daily,
            daily: None,
            texto_daily: String::new(),
            focar_daily: false,
            erro_daily: None,
            sprint: None,
            texto_sprint: String::new(),
            periodo: Periodo::Dias14,
            de: String::new(),
            ate: String::new(),
            erro_sprint: None,
            miniaturas: HashMap::new(),
            uso: VecDeque::new(),
            visor: None,
            salvando: false,
            suja: false,
            conectado: true,
        }
    }
}

/// Faz o pedido numa thread e acorda a tela quando a resposta chega.
fn em_segundo_plano(envio: &Sender<Resposta>, ctx: &egui::Context, pedido: impl FnOnce() -> Resposta + Send + 'static) {
    let envio = envio.clone();
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let _ = envio.send(pedido());
        ctx.request_repaint();
    });
}

/// "25/09/2026" vira "2026-09-25"; outra coisa é recusada aqui mesmo.
fn data_da_tela(texto: &str) -> Option<String> {
    let partes: Vec<&str> = texto.trim().split('/').collect();
    match partes[..] {
        [d, m, a] if d.len() == 2 && m.len() == 2 && a.len() == 4 && texto.trim().chars().all(|c| c.is_ascii_digit() || c == '/') => {
            Some(format!("{a}-{m}-{d}"))
        }
        _ => None,
    }
}

/// Decodifica um PNG e, para miniatura, reduz a altura (média de cada bloco).
fn decodificar(png: &[u8], altura_maxima: Option<u32>) -> Result<ColorImage, String> {
    let mut decodificador = png::Decoder::new(std::io::Cursor::new(png));
    decodificador.set_transformations(png::Transformations::normalize_to_color8());
    let mut leitor = decodificador.read_info().map_err(|e| e.to_string())?;
    let mut buffer = vec![0; leitor.output_buffer_size().ok_or("imagem grande demais")?];
    let info = leitor.next_frame(&mut buffer).map_err(|e| e.to_string())?;
    let (largura, altura) = (info.width as usize, info.height as usize);
    let canais = match info.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Grayscale => 1,
        png::ColorType::Indexed => return Err("paleta não expandida".into()),
    };
    let pixel = |x: usize, y: usize| -> [u8; 4] {
        let i = y * info.line_size + x * canais;
        match canais {
            4 => [buffer[i], buffer[i + 1], buffer[i + 2], buffer[i + 3]],
            3 => [buffer[i], buffer[i + 1], buffer[i + 2], 255],
            2 => [buffer[i], buffer[i], buffer[i], buffer[i + 1]],
            _ => [buffer[i], buffer[i], buffer[i], 255],
        }
    };
    let fator = altura_maxima.map_or(1, |m| altura.div_ceil(m as usize).max(1));
    let (nl, na) = ((largura / fator).max(1), (altura / fator).max(1));
    let mut rgba = Vec::with_capacity(nl * na * 4);
    for y in 0..na {
        for x in 0..nl {
            let mut soma = [0u32; 4];
            let mut n = 0;
            for dy in 0..fator {
                for dx in 0..fator {
                    let (sx, sy) = (x * fator + dx, y * fator + dy);
                    if sx < largura && sy < altura {
                        for (s, v) in soma.iter_mut().zip(pixel(sx, sy)) {
                            *s += v as u32;
                        }
                        n += 1;
                    }
                }
            }
            rgba.extend(soma.map(|s| (s / n.max(1)) as u8));
        }
    }
    Ok(ColorImage::from_rgba_unmultiplied([nl, na], &rgba))
}

/// Tipo de um item da linha do tempo → o ponto da tabela de estados.
fn ponto_do_tipo(tipo: &str) -> EstadoVisual {
    match tipo {
        "concluiu" => EstadoVisual::Concluiu,
        "erro" => EstadoVisual::Erro,
        "sessao_aberta" => EstadoVisual::Trabalhando,
        "sessao_aguardando" => EstadoVisual::SuaVez,
        // Terminal aberto e parado: anel cinza, como "Parado" no cartão.
        "sessao_parada" => EstadoVisual::Parado,
        "interrompido" => EstadoVisual::Interrompido,
        _ => EstadoVisual::Terminou,
    }
}

/// Uma linha de texto cortada com "…"; diz se cortou.
fn texto_cortado(pintor: &egui::Painter, pos: Pos2, texto: &str, fonte: FontId, cor: Color32, largura: f32) -> bool {
    let mut trabalho = LayoutJob::simple_singleline(texto.to_owned(), fonte, cor);
    trabalho.wrap = TextWrapping { max_width: largura.max(10.0), max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
    let galeria = pintor.layout_job(trabalho);
    let cortou = galeria.elided;
    pintor.galley(pos - vec2(0.0, galeria.size().y / 2.0), galeria, cor);
    cortou
}

enum Fileira {
    Cabecalho(usize),
    Item(usize, usize),
    Carregando,
    Inicio,
}

impl Linha {
    /// Ctrl+Shift+D: a linha do tempo com o painel aberto na daily, foco no texto.
    pub fn abrir_daily(&mut self) {
        self.painel_aberto = true;
        self.painel_decidido = true;
        self.aba = Aba::Daily;
        self.focar_daily = true;
    }

    /// Algo mudou no perfil. No topo, a primeira página vem de novo; rolado
    /// para baixo, a pílula conta as novidades e a lista não pula.
    pub fn novidade(&mut self, perfil: i64, projeto: Option<i64>, ctx: &egui::Context) {
        if self.chave != Some((perfil, projeto)) {
            return;
        }
        if self.no_topo {
            self.pedir_pagina(ctx, true);
        } else {
            self.novidades += 1;
        }
        self.pedir_resumos(ctx, false);
    }

    /// Algo mudou com a linha do tempo fora da tela (no quadro ou numa
    /// tarefa). Nada é pedido agora; ao voltar, ela busca de novo.
    pub fn marcar_suja(&mut self) {
        self.suja = true;
    }

    /// Um agente mudou de estado: a sessão aberta no topo diz o estado de agora.
    /// Não é novidade (não aparece na pílula); só atualiza quem está no topo.
    pub fn estado_mudou(&mut self, perfil: i64, projeto: Option<i64>, ctx: &egui::Context) {
        if self.chave == Some((perfil, projeto)) && self.no_topo {
            self.pedir_pagina(ctx, true);
        }
    }

    fn pedir_pagina(&mut self, ctx: &egui::Context, primeira: bool) {
        let Some(chave) = self.chave else { return };
        if self.carregando && !primeira {
            return;
        }
        self.carregando = true;
        let antes = if primeira { 0 } else { self.proximo };
        em_segundo_plano(&self.canal.0, ctx, move || Resposta::Pagina { chave, primeira, resultado: api::linha_do_tempo(chave.0, chave.1, antes) });
    }

    /// Daily e sprint de novo; `trocar_texto` descarta a edição.
    fn pedir_resumos(&mut self, ctx: &egui::Context, trocar_texto: bool) {
        let Some(chave) = self.chave else { return };
        if trocar_texto {
            self.daily = None;
            self.sprint = None;
        }
        em_segundo_plano(&self.canal.0, ctx, move || Resposta::Daily { chave, resultado: api::daily(chave.0, chave.1) });
        self.pedir_sprint(ctx);
    }

    fn periodo_pedido(&mut self) -> Option<api::PeriodoSprint> {
        self.erro_sprint = None;
        match self.periodo {
            Periodo::Dias7 => Some(api::PeriodoSprint::Ultimos(7)),
            Periodo::Dias14 => Some(api::PeriodoSprint::Ultimos(14)),
            Periodo::Mes => Some(api::PeriodoSprint::MesAtual),
            Periodo::Escolher => match (data_da_tela(&self.de), data_da_tela(&self.ate)) {
                (Some(de), Some(ate)) => Some(api::PeriodoSprint::Datas(de, ate)),
                _ => {
                    if !self.de.is_empty() && !self.ate.is_empty() {
                        self.erro_sprint = Some("Use datas no formato DD/MM/AAAA.".into());
                    }
                    None
                }
            },
        }
    }

    fn pedir_sprint(&mut self, ctx: &egui::Context) {
        let Some(chave) = self.chave else { return };
        let Some(periodo) = self.periodo_pedido() else { return };
        em_segundo_plano(&self.canal.0, ctx, move || Resposta::Sprint { chave, resultado: api::sprint(chave.0, chave.1, &periodo) });
    }

    fn pedir_imagem(&mut self, ctx: &egui::Context, id: i64, grande: bool) {
        em_segundo_plano(&self.canal.0, ctx, move || Resposta::Miniatura {
            id,
            grande,
            resultado: api::ler_anexo(id).and_then(|png| decodificar(&png, (!grande).then_some(ALTURA_MINIATURA))),
        });
    }

    /// Miniatura do cache; pede se ainda não veio. O cache guarda as 64 mais
    /// recentes: rolar uma sprint inteira não enche a memória de texturas.
    fn miniatura(&mut self, ctx: &egui::Context, id: i64) -> &Miniatura {
        if let std::collections::hash_map::Entry::Vacant(e) = self.miniaturas.entry(id) {
            e.insert(Miniatura::Carregando);
            self.pedir_imagem(ctx, id, false);
        }
        if self.uso.back() != Some(&id) {
            self.uso.retain(|x| *x != id);
            self.uso.push_back(id);
        }
        while self.uso.len() > MAX_MINIATURAS {
            if let Some(velha) = self.uso.pop_front() {
                self.miniaturas.remove(&velha);
            }
        }
        &self.miniaturas[&id]
    }

    fn receber(&mut self, ctx: &egui::Context, acoes: &mut Vec<Acao>) {
        while let Ok(r) = self.canal.1.try_recv() {
            match r {
                Resposta::Pagina { chave, primeira, resultado } if Some(chave) == self.chave => {
                    self.carregando = false;
                    match resultado {
                        Ok(pagina) => {
                            self.carregou = true;
                            self.erro = None;
                            self.proximo = pagina.proximo;
                            self.criado_em = pagina.perfil_criado_em;
                            if primeira {
                                self.dias = pagina.dias;
                                self.novidades = 0;
                            } else {
                                for dia in pagina.dias {
                                    // Um dia pode vir dividido entre duas páginas.
                                    match self.dias.last_mut() {
                                        Some(ultimo) if ultimo.dia == dia.dia => ultimo.itens.extend(dia.itens),
                                        _ => self.dias.push(dia),
                                    }
                                }
                            }
                        }
                        Err(e) => self.erro = Some(e),
                    }
                }
                // Sem o núcleo, a faixa do topo já avisa: o painel guarda o que tinha.
                Resposta::Daily { resultado: Err(e), .. } | Resposta::Sprint { resultado: Err(e), .. } if api::erro_de_conexao(&e) => {}
                Resposta::Daily { chave, resultado } if Some(chave) == self.chave => match resultado {
                    Ok(d) => {
                        // Só troca o texto se você não estava editando.
                        if self.daily.as_ref().is_none_or(|antigo| antigo.texto == self.texto_daily) {
                            self.texto_daily = d.texto.clone();
                        }
                        self.daily = Some(d);
                        self.erro_daily = None;
                    }
                    Err(e) => self.erro_daily = Some(if api::erro_inesperado(&e) { "Não consegui montar a daily. Tente de novo.".into() } else { e }),
                },
                Resposta::Sprint { chave, resultado } if Some(chave) == self.chave => match resultado {
                    Ok(s) => {
                        if self.sprint.as_ref().is_none_or(|antigo| antigo.texto == self.texto_sprint || antigo.periodo != s.periodo) {
                            self.texto_sprint = s.texto.clone();
                        }
                        self.sprint = Some(s);
                        self.erro_sprint = None;
                    }
                    // O núcleo explica (datas invertidas, mais de 92 dias): aparece abaixo dos campos.
                    Err(e) => self.erro_sprint = Some(if api::erro_inesperado(&e) { "Não consegui montar a sprint. Tente de novo.".into() } else { e }),
                },
                Resposta::Miniatura { id, grande: true, resultado } => {
                    if let Some(v) = self.visor.as_mut().filter(|v| v.anexo == id) {
                        v.imagem = Some(resultado.map(|img| ctx.load_texture(format!("captura-{id}"), img, TextureOptions::LINEAR)));
                    }
                }
                Resposta::Miniatura { id, grande: false, resultado } => {
                    if self.miniaturas.contains_key(&id) {
                        let m = match resultado {
                            Ok(img) => Miniatura::Pronta(ctx.load_texture(format!("miniatura-{id}"), img, TextureOptions::LINEAR)),
                            Err(_) => Miniatura::Falhou,
                        };
                        self.miniaturas.insert(id, m);
                    }
                }
                Resposta::Salvo(resultado) => {
                    self.salvando = false;
                    match resultado {
                        Ok(caminho) => acoes.push(Acao::Avisar(TipoAviso::Neutro, format!("Salvo em {caminho}"))),
                        Err(e) if e.is_empty() => {}
                        Err(e) => acoes.push(Acao::Avisar(TipoAviso::Erro, format!("Não consegui salvar: {e}"))),
                    }
                }
                Resposta::Removido(resultado) => match resultado {
                    Ok(()) => {
                        self.visor = None;
                        acoes.push(Acao::Avisar(TipoAviso::Neutro, "Captura removida".into()));
                        self.pedir_pagina(ctx, true);
                        self.pedir_sprint(ctx);
                    }
                    Err(e) => acoes.push(Acao::Avisar(TipoAviso::Erro, format!("Não consegui remover: {e}"))),
                },
                // Resposta de um escopo que já não está na tela.
                _ => {}
            }
        }
    }

    /// Desenha a linha do tempo e o painel. `projetos` são os nomes no escopo.
    pub fn mostrar(&mut self, ui: &mut egui::Ui, perfil: i64, projeto: Option<i64>, projetos: &[String]) -> Vec<Acao> {
        let ctx = ui.ctx().clone();
        let mut acoes = Vec::new();
        let chave = (perfil, projeto);
        if self.chave != Some(chave) {
            let painel = (self.painel_aberto, self.painel_decidido, self.aba, self.focar_daily, self.periodo);
            *self = Linha { canal: std::mem::replace(&mut self.canal, mpsc::channel()), ..Linha::default() };
            (self.painel_aberto, self.painel_decidido, self.aba, self.focar_daily, self.periodo) = painel;
            self.chave = Some(chave);
            self.suja = false;
            self.pedir_pagina(&ctx, true);
            self.pedir_resumos(&ctx, true);
        } else if self.conectado && std::mem::take(&mut self.suja) {
            // Voltou para a linha do tempo depois de eventos que ela não viu.
            self.carregando = false;
            if self.no_topo {
                self.pedir_pagina(&ctx, true);
            } else {
                self.novidades = self.novidades.max(1);
            }
            self.pedir_resumos(&ctx, false);
        }
        self.receber(&ctx, &mut acoes);

        let area = ui.available_rect_before_wrap();
        // Em telas estreitas o painel começa fechado; a lista nunca fica com menos de 480.
        if !self.painel_decidido {
            self.painel_decidido = true;
            self.painel_aberto = area.width() >= 1000.0;
        }
        let com_painel = self.painel_aberto && area.width() >= 480.0 + 16.0 + LARGURA_PAINEL;
        let lista = if com_painel { Rect::from_min_max(area.min, pos2(area.max.x - LARGURA_PAINEL - 16.0, area.max.y)) } else { area };
        let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(lista));
        self.lista(&mut filho, projeto, &mut acoes);
        if com_painel {
            let caixa = Rect::from_min_max(pos2(area.max.x - LARGURA_PAINEL, area.min.y), area.max);
            let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(caixa));
            self.painel(&mut filho, projetos, &mut acoes);
        }
        ui.allocate_rect(area, Sense::hover());
        self.mostrar_visor(&ctx, &mut acoes);
        acoes
    }

    fn lista(&mut self, ui: &mut egui::Ui, projeto: Option<i64>, acoes: &mut Vec<Acao>) {
        let p = cores();
        let ctx = ui.ctx().clone();
        if let Some(e) = &self.erro
            && self.dias.is_empty()
            && !api::erro_de_conexao(e)
        {
            ui.label(RichText::new("Não consegui ler a linha do tempo. Tente de novo.").color(p.erro).size(12.5));
            return;
        }
        if self.carregou && self.dias.is_empty() {
            let (titulo, texto, botao) = match projeto {
                Some(_) => ("Nada neste projeto ainda", "Quando você criar tarefas e abrir agentes aqui, tudo aparece dia a dia.", "Ver todos os projetos"),
                None => ("Nada aconteceu ainda neste perfil", "Quando você criar tarefas e abrir agentes, tudo aparece aqui, dia a dia.", "Ir para o quadro"),
            };
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() * 0.25);
                ui.allocate_ui(vec2(460.0, 0.0), |ui| {
                    tema::moldura_janela().show(ui, |ui| {
                        ui.set_width(412.0);
                        tema::cabecalho(ui, titulo, texto);
                        ui.add_space(16.0);
                        if tema::botao_secundario(ui, botao).clicked() {
                            acoes.push(if projeto.is_some() { Acao::VerTodos } else { Acao::IrParaQuadro });
                        }
                    });
                });
            });
            return;
        }

        // Fileiras com alturas fixas, calculadas sem montar texto.
        let mut fileiras: Vec<(f32, f32, Fileira)> = Vec::new();
        let mut y = 0.0;
        for (d, dia) in self.dias.iter().enumerate() {
            fileiras.push((y, CABECALHO, Fileira::Cabecalho(d)));
            y += CABECALHO + RESPIRO_BLOCO;
            for (i, item) in dia.itens.iter().enumerate() {
                let h = if item.anexos.is_empty() { ITEM } else { ITEM_COM_IMAGENS };
                fileiras.push((y, h, Fileira::Item(d, i)));
                y += h;
            }
            y += RESPIRO_BLOCO + ENTRE_DIAS;
        }
        if self.proximo > 0 {
            fileiras.push((y, ITEM, Fileira::Carregando));
            y += ITEM;
        } else if self.carregou {
            fileiras.push((y, LINHA_ESPECIAL, Fileira::Inicio));
            y += LINHA_ESPECIAL;
        }
        let total = y;

        let mut rolagem = egui::ScrollArea::vertical().id_salt("linha-do-tempo").auto_shrink(false);
        if std::mem::take(&mut self.ir_ao_topo) {
            rolagem = rolagem.vertical_scroll_offset(0.0);
        }
        let mut pedir_mais = false;
        let saida = rolagem.show_viewport(ui, |ui, janela| {
            ui.set_height(total);
            let origem = ui.max_rect().min;
            let largura = ui.available_width();
            // Fundo de cada dia (como as colunas do quadro), só dos visíveis.
            for (d, dia) in self.dias.iter().enumerate() {
                let Some(inicio) = fileiras.iter().find(|f| matches!(f.2, Fileira::Cabecalho(x) if x == d)).map(|f| f.0) else { continue };
                let altura: f32 = dia.itens.iter().map(|i| if i.anexos.is_empty() { ITEM } else { ITEM_COM_IMAGENS }).sum::<f32>() + 2.0 * RESPIRO_BLOCO;
                let topo = inicio + CABECALHO;
                if topo + altura >= janela.min.y && topo <= janela.max.y {
                    let caixa = Rect::from_min_size(origem + vec2(0.0, topo), vec2(largura, altura));
                    let fundo = Color32::from_rgba_unmultiplied(p.superficie.r(), p.superficie.g(), p.superficie.b(), 190);
                    ui.painter().rect_filled(caixa, CornerRadius::same(tema::RAIO_SUPERFICIE), fundo);
                }
            }
            for (fy, fh, fileira) in &fileiras {
                if fy + fh < janela.min.y || *fy > janela.max.y {
                    continue;
                }
                let rect = Rect::from_min_size(origem + vec2(0.0, *fy), vec2(largura, *fh));
                match fileira {
                    Fileira::Cabecalho(d) => {
                        let dia = &self.dias[*d];
                        let base = pos2(rect.left() + 4.0, rect.top() + 12.0 + 16.0);
                        let titulo = ui.painter().layout_no_wrap(dia.titulo.clone(), forte(15.0), p.texto);
                        let largura_titulo = titulo.size().x;
                        ui.painter().galley(base - vec2(0.0, titulo.size().y / 2.0), titulo, p.texto);
                        if !dia.resumo.is_empty() {
                            ui.painter().text(
                                base + vec2(largura_titulo + 12.0, 0.0),
                                egui::Align2::LEFT_CENTER,
                                &dia.resumo,
                                FontId::proportional(12.5),
                                p.suave,
                            );
                        }
                    }
                    Fileira::Item(d, i) => {
                        let ultimo = *i + 1 == self.dias[*d].itens.len();
                        if let Some(a) = self.item(ui, rect.shrink2(vec2(4.0, 0.0)), *d, *i, *i == 0, ultimo, projeto.is_none(), &ctx) {
                            acoes.push(a);
                        }
                    }
                    Fileira::Carregando => {
                        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "Carregando dias anteriores…", FontId::proportional(12.5), p.suave);
                        pedir_mais = true;
                    }
                    Fileira::Inicio => {
                        let texto = format!("Início do histórico deste perfil (criado em {}).", self.criado_em);
                        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, texto, FontId::proportional(12.5), p.suave);
                    }
                }
            }
        });
        self.no_topo = saida.state.offset.y < 4.0;
        if self.no_topo && self.novidades > 0 {
            self.novidades = 0;
            self.pedir_pagina(&ctx, true);
        }
        if pedir_mais && !self.carregando {
            self.pedir_pagina(&ctx, false);
        }
        // Pílula de novidades: estática, some ao clicar ou ao voltar ao topo.
        if self.novidades > 0 {
            let texto = if self.novidades == 1 { "1 novidade · mostrar".to_string() } else { format!("{} novidades · mostrar", self.novidades) };
            let pos = saida.inner_rect.center_top() + vec2(0.0, 10.0);
            egui::Area::new(Id::new("pilula-novidades")).order(egui::Order::Foreground).pivot(egui::Align2::CENTER_TOP).fixed_pos(pos).show(&ctx, |ui| {
                egui::Frame::new().shadow(tema::sombra(4, 12)).corner_radius(17).show(ui, |ui| {
                    if tema::botao_principal(ui, &texto, true).clicked() {
                        self.ir_ao_topo = true;
                        self.novidades = 0;
                        self.pedir_pagina(&ctx, true);
                    }
                });
            });
        }
    }

    /// Um item: hora, trilho e ponto, texto, projeto e miniaturas.
    #[allow(clippy::too_many_arguments)]
    fn item(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        d: usize,
        i: usize,
        primeiro: bool,
        ultimo: bool,
        com_projeto: bool,
        ctx: &egui::Context,
    ) -> Option<Acao> {
        let p = cores();
        let item = self.dias[d].itens[i].clone();
        let linha = Rect::from_min_size(rect.min, vec2(rect.width(), ITEM));
        let clicavel = item.tarefa_id != 0 && !item.removida;
        let resposta = ui.interact(linha, Id::new(("item-linha", item.evento, d, i)), if clicavel { Sense::click() } else { Sense::hover() });
        let pintor = ui.painter().clone();
        if clicavel && resposta.hovered() {
            pintor.rect_filled(linha, CornerRadius::same(tema::RAIO_CONTROLE), p.realce.gamma_multiply(0.6));
        }
        let meio = linha.center().y;
        pintor.text(pos2(rect.left() + 12.0, meio), egui::Align2::LEFT_CENTER, &item.hora, FontId::monospace(12.0), p.suave);
        // Trilho ligando os pontos do dia, do primeiro ao último item.
        let x_trilho = rect.left() + 68.0;
        let (topo, base) = (if primeiro { meio } else { rect.top() }, if ultimo { meio } else { rect.bottom() });
        if topo < base {
            // Cerca de 1,4:1 sobre o bloco do dia nos três temas, sem competir com os pontos.
            let trilho = tema::misturar(p.superficie, p.suave, if tema::claro() { 0.3 } else { 0.22 });
            pintor.line_segment([pos2(x_trilho, topo), pos2(x_trilho, base)], Stroke::new(2.0, trilho));
        }
        let estado = ponto_do_tipo(&item.tipo);
        tema::ponto(&pintor, pos2(x_trilho, meio), 4.0, estado);
        let mut direita = rect.right() - 12.0;
        if com_projeto && !item.projeto.is_empty() {
            let largura = pintor.layout_no_wrap(item.projeto.clone(), tema::fonte_etiqueta(), p.suave).size().x + 12.0;
            direita -= largura;
            tema::etiqueta(&pintor, pos2(direita, meio - 9.5), &item.projeto, tema::fonte_etiqueta(), p.suave);
            direita -= 12.0;
        }
        if item.tipo.starts_with("sessao_a") {
            let cor = estado.cor();
            let largura = pintor.layout_no_wrap("agora".into(), tema::fonte_etiqueta(), cor).size().x + 12.0;
            direita -= largura;
            tema::etiqueta(&pintor, pos2(direita, meio - 9.5), "agora", tema::fonte_etiqueta(), cor);
            direita -= 12.0;
        }
        let x_texto = rect.left() + 84.0;
        let cortou = texto_cortado(&pintor, pos2(x_texto, meio), &item.texto, FontId::proportional(13.5), p.texto, direita - x_texto);
        let resposta = if !clicavel && item.tarefa_id != 0 {
            resposta.on_hover_text("tarefa removida")
        } else if cortou {
            resposta.on_hover_text(&item.texto)
        } else {
            resposta
        };
        let resposta = if clicavel { resposta.on_hover_cursor(egui::CursorIcon::PointingHand) } else { resposta };
        let mut acao = None;
        if resposta.clicked() {
            acao = Some(Acao::AbrirTarefa { tarefa: item.tarefa_id, agente: (item.agente_id != 0).then_some(item.agente_id) });
        }

        // Miniaturas: até 4, a quinta vira "+N".
        let mut x = x_texto;
        let y = rect.top() + ITEM;
        for (n, &anexo) in item.anexos.iter().enumerate() {
            if n == 4 {
                let caixa = Rect::from_min_size(pos2(x, y), vec2(64.0, 64.0));
                pintor.rect_filled(caixa, CornerRadius::same(tema::RAIO_CONTROLE), p.superficie_alta);
                pintor.text(caixa.center(), egui::Align2::CENTER_CENTER, format!("+{}", item.anexos.len() - 4), FontId::proportional(13.0), p.suave);
                break;
            }
            let largura = match self.miniatura(ctx, anexo) {
                Miniatura::Pronta(t) => (64.0 * t.size_vec2().x / t.size_vec2().y.max(1.0)).clamp(32.0, 160.0),
                _ => 114.0,
            };
            let caixa = Rect::from_min_size(pos2(x, y), vec2(largura, 64.0));
            if self.desenhar_miniatura(ui, caixa, anexo) {
                self.abrir_visor(ctx, anexo, &item.texto, &item.hora, item.tarefa_id);
            }
            x += largura + 8.0;
        }
        acao
    }

    /// Desenha a miniatura (ou o lugar dela) e diz se foi clicada.
    fn desenhar_miniatura(&mut self, ui: &mut egui::Ui, caixa: Rect, anexo: i64) -> bool {
        let p = cores();
        let resposta = ui.interact(caixa, Id::new(("miniatura", anexo, caixa.min.x as i32, caixa.min.y as i32)), Sense::click());
        let raio = CornerRadius::same(tema::RAIO_CONTROLE);
        match self.miniaturas.get(&anexo) {
            Some(Miniatura::Pronta(t)) => {
                // Preenche a caixa cortando o excesso, sem distorcer.
                let tamanho = t.size_vec2();
                let escala = (caixa.width() / tamanho.x).max(caixa.height() / tamanho.y);
                let visivel = vec2(caixa.width() / (tamanho.x * escala), caixa.height() / (tamanho.y * escala));
                egui::Image::new(t).uv(Rect::from_center_size(pos2(0.5, 0.5), visivel)).corner_radius(raio).paint_at(ui, caixa);
            }
            // Sem spinner: um retângulo parado não pede redesenho.
            Some(Miniatura::Falhou) => {
                ui.painter().rect_filled(caixa, raio, p.realce);
                ui.painter().text(caixa.center(), egui::Align2::CENTER_CENTER, "imagem indisponível", FontId::proportional(11.0), p.suave);
            }
            _ => {
                ui.painter().rect_filled(caixa, raio, p.realce);
            }
        }
        ui.painter().rect_stroke(caixa, raio, Stroke::new(1.0, p.borda), StrokeKind::Inside);
        resposta.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
    }

    fn abrir_visor(&mut self, ctx: &egui::Context, anexo: i64, texto: &str, quando: &str, tarefa: i64) {
        self.visor = Some(Visor { anexo, texto: texto.to_string(), quando: quando.to_string(), tarefa, imagem: None, confirmar: false });
        self.pedir_imagem(ctx, anexo, true);
    }

    fn mostrar_visor(&mut self, ctx: &egui::Context, acoes: &mut Vec<Acao>) {
        let Some(visor) = &mut self.visor else { return };
        let p = cores();
        let tela = ctx.content_rect();
        let maximo = vec2((tela.width() * 0.9).min(1200.0), (tela.height() * 0.9 - 160.0).min(700.0));
        let (mut fechar, mut remover, mut abrir) = (false, false, false);
        let modal = egui::Modal::new(Id::new("visor-captura"))
            .frame(tema::moldura_janela())
            .backdrop_color(Color32::from_black_alpha(if tema::claro() { 60 } else { 140 }))
            .show(ctx, |ui| {
                ui.label(tema::texto_forte(&visor.texto, 15.0).color(p.texto));
                ui.label(RichText::new(&visor.quando).color(p.suave).size(12.5));
                ui.add_space(10.0);
                match &visor.imagem {
                    Some(Ok(t)) => {
                        let tamanho = t.size_vec2();
                        let escala = (maximo.x / tamanho.x).min(maximo.y / tamanho.y).min(1.0);
                        ui.add(egui::Image::new(t).fit_to_exact_size(tamanho * escala).corner_radius(CornerRadius::same(tema::RAIO_CONTROLE)));
                    }
                    Some(Err(_)) => {
                        let (r, _) = ui.allocate_exact_size(vec2(480.0, 200.0), Sense::hover());
                        ui.painter().rect_filled(r, CornerRadius::same(tema::RAIO_CONTROLE), p.realce);
                        ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, "imagem indisponível", FontId::proportional(12.0), p.suave);
                    }
                    None => {
                        let (r, _) = ui.allocate_exact_size(vec2(480.0, 270.0), Sense::hover());
                        ui.painter().rect_filled(r, CornerRadius::same(tema::RAIO_CONTROLE), p.realce);
                    }
                }
                ui.add_space(16.0);
                if visor.confirmar {
                    ui.label(RichText::new("Remover esta captura? Ela sai da linha do tempo e da sprint; o arquivo é apagado.").color(p.texto).size(13.5));
                    ui.add_space(10.0);
                }
                ui.horizontal(|ui| {
                    if visor.confirmar {
                        if tema::botao_secundario(ui, "Cancelar").clicked() {
                            visor.confirmar = false;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            remover = tema::botao_principal(ui, "Remover", true).clicked();
                        });
                        return;
                    }
                    if visor.tarefa != 0 {
                        abrir = tema::botao_secundario(ui, "Abrir tarefa").clicked();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        fechar = tema::botao_secundario(ui, "Fechar").clicked();
                        ui.add_space(8.0);
                        if tema::botao_secundario(ui, "Remover").clicked() {
                            visor.confirmar = true;
                        }
                    });
                });
            });
        // Na confirmação, Enter remove e Esc volta (sem fechar o visor).
        if visor.confirmar {
            let (enter, esc) = ctx.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
            if enter {
                remover = true;
            } else if esc {
                visor.confirmar = false;
                ctx.request_repaint();
                return;
            }
        }
        let (anexo, tarefa) = (visor.anexo, visor.tarefa);
        if abrir {
            acoes.push(Acao::AbrirTarefa { tarefa, agente: None });
            fechar = true;
        }
        if remover {
            em_segundo_plano(&self.canal.0, ctx, move || Resposta::Removido(api::remover_anexo(anexo)));
        }
        if fechar || modal.should_close() {
            self.visor = None;
        }
    }

    fn painel(&mut self, ui: &mut egui::Ui, projetos: &[String], acoes: &mut Vec<Acao>) {
        let p = cores();
        let ctx = ui.ctx().clone();
        egui::Frame::new()
            .fill(p.superficie_alta)
            .stroke(Stroke::new(1.0, p.borda))
            .corner_radius(CornerRadius::same(tema::RAIO_SUPERFICIE))
            .inner_margin(egui::Margin::same(16))
            .show(ui, |ui| {
                ui.set_min_size(ui.available_size());
                let largura = ui.available_width();
                ui.horizontal(|ui| {
                    ui.set_height(34.0);
                    let atual = if self.aba == Aba::Daily { 0 } else { 1 };
                    if let Some(i) = tema::segmentado(ui, &["Daily", "Sprint"], atual) {
                        self.aba = if i == 0 { Aba::Daily } else { Aba::Sprint };
                        if self.aba == Aba::Daily {
                            self.focar_daily = true;
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if tema::botao_icone(ui, tema::Icone::Fechar, 28.0).on_hover_text("Fechar painel").clicked() {
                            self.painel_aberto = false;
                        }
                    });
                });
                ui.add_space(14.0);
                egui::ScrollArea::vertical().id_salt("painel-resumo").auto_shrink(false).show(ui, |ui| {
                    ui.set_width(largura);
                    match self.aba {
                        Aba::Daily => self.aba_daily(ui, acoes),
                        Aba::Sprint => self.aba_sprint(ui, &ctx, projetos, acoes),
                    }
                });
            });
    }

    fn aba_daily(&mut self, ui: &mut egui::Ui, acoes: &mut Vec<Acao>) {
        let p = cores();
        if let Some(e) = &self.erro_daily {
            ui.label(RichText::new(format!("Não consegui montar a daily: {e}")).color(p.erro).size(12.5));
            return;
        }
        let Some(daily) = self.daily.clone() else {
            ui.label(RichText::new("Montando a daily…").color(p.suave).size(12.5));
            return;
        };
        ui.label(RichText::new(&daily.periodo).color(p.suave).size(12.5));
        ui.add_space(10.0);
        let id = Id::new("texto-daily");
        let resposta = tema::campo_multilinha(ui, &mut self.texto_daily, 8, 300.0, id, daily.vazio);
        // Foco no texto uma vez só (pedir em todo quadro trava os eventos).
        if std::mem::take(&mut self.focar_daily) && !daily.vazio {
            resposta.request_focus();
        }
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if tema::botao_principal(ui, "Copiar", !daily.vazio && self.conectado).on_hover_text("Copiar o texto da daily").clicked() {
                ui.ctx().copy_text(self.texto_daily.clone());
                acoes.push(Acao::Avisar(TipoAviso::Neutro, "Copiado".into()));
            }
            if self.texto_daily != daily.texto {
                ui.add_space(8.0);
                if tema::botao_secundario(ui, "Refazer texto").clicked() {
                    self.texto_daily = daily.texto.clone();
                }
            }
        });
        ui.add_space(20.0);
        let mut partes: Vec<&api::ParteDaily> = Vec::new();
        if let Some(o) = &daily.ontem {
            partes.push(o);
        }
        partes.push(&daily.hoje);
        for parte in partes {
            if parte.blocos.is_empty() {
                continue;
            }
            ui.label(tema::texto_forte(&parte.titulo, 13.0).color(p.texto));
            ui.add_space(6.0);
            for bloco in &parte.blocos {
                ui.label(tema::texto_forte(&bloco.titulo, 12.0).color(p.suave));
                ui.add_space(4.0);
                for item in &bloco.itens {
                    if let Some(a) = item_resumo(ui, item) {
                        acoes.push(a);
                    }
                }
                if bloco.mais > 0 {
                    ui.label(RichText::new(format!("e mais {}", bloco.mais)).color(p.suave).size(12.0));
                }
                ui.add_space(14.0);
            }
        }
    }

    fn aba_sprint(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, projetos: &[String], acoes: &mut Vec<Acao>) {
        let p = cores();
        let largura = ui.available_width();
        let opcoes = [Periodo::Dias7, Periodo::Dias14, Periodo::Mes, Periodo::Escolher];
        let atual = opcoes.iter().position(|o| *o == self.periodo).unwrap_or(1);
        let escolha = tema::segmentado_com_largura(ui, &["7 dias", "14 dias", "Este mês", "Escolher"], atual, Some(largura));
        if let Some(i) = escolha
            && opcoes[i] != self.periodo
        {
            self.periodo = opcoes[i];
            self.sprint = None;
            self.pedir_sprint(ctx);
        }
        if self.periodo == Periodo::Escolher {
            ui.add_space(10.0);
            let (de, ate) = (self.de.clone(), self.ate.clone());
            ui.horizontal(|ui| {
                let metade = (largura - 8.0) / 2.0;
                ui.vertical(|ui| {
                    ui.set_width(metade);
                    tema::campo(ui, "De", &mut self.de, "DD/MM/AAAA");
                });
                ui.add_space(8.0 - ui.spacing().item_spacing.x);
                ui.vertical(|ui| {
                    ui.set_width(metade);
                    tema::campo(ui, "Até", &mut self.ate, "DD/MM/AAAA");
                });
            });
            // Pede quando as duas datas estão completas e mudaram.
            if (de != self.de || ate != self.ate) && self.de.trim().len() == 10 && self.ate.trim().len() == 10 {
                self.sprint = None;
                self.pedir_sprint(ctx);
            }
        }
        if let Some(e) = &self.erro_sprint {
            ui.add_space(4.0);
            ui.label(RichText::new(e).color(p.erro).size(12.5));
        }
        ui.add_space(12.0);
        let Some(sprint) = self.sprint.clone() else {
            if self.erro_sprint.is_none() && self.periodo != Periodo::Escolher {
                ui.label(RichText::new("Montando a sprint…").color(p.suave).size(12.5));
            }
            return;
        };
        ui.label(RichText::new(&sprint.periodo).color(p.suave).size(12.5));
        ui.add_space(8.0);
        tema::campo_multilinha(ui, &mut self.texto_sprint, 10, 360.0, Id::new("texto-sprint"), sprint.vazio);
        ui.add_space(10.0);
        let ativo = !sprint.vazio && self.conectado;
        ui.horizontal(|ui| {
            if tema::botao_principal(ui, "Copiar texto", ativo).clicked() {
                ui.ctx().copy_text(self.texto_sprint.clone());
                acoes.push(Acao::Avisar(TipoAviso::Neutro, "Copiado".into()));
            }
            ui.add_space(8.0);
            if tema::botao_secundario_com(ui, "Copiar em Markdown", ativo).clicked() {
                ui.ctx().copy_text(sprint.markdown.clone());
                acoes.push(Acao::Avisar(TipoAviso::Neutro, "Copiado em Markdown".into()));
            }
        });
        ui.add_space(8.0);
        let n = sprint.capturas.len();
        // O rótulo diz o que sai do computador junto com o texto.
        let rotulo = match (self.salvando, n) {
            (true, _) => "Salvando…".to_string(),
            (_, 0) => "Salvar…".to_string(),
            (_, 1) => "Salvar com 1 captura…".to_string(),
            (_, n) => format!("Salvar com {n} capturas…"),
        };
        if tema::botao_secundario_com(ui, &rotulo, ativo && !self.salvando).clicked() {
            self.salvar(ctx, &sprint, projetos);
        }
        if n > 0 {
            ui.add_space(20.0);
            ui.label(tema::texto_forte(format!("Capturas · {n}"), 12.0).color(p.suave));
            ui.add_space(6.0);
            let (lado_x, lado_y) = (((largura - 16.0) / 3.0).min(124.0), 70.0);
            for linha in sprint.capturas.chunks(3) {
                ui.horizontal(|ui| {
                    for c in linha {
                        let (caixa, _) = ui.allocate_exact_size(vec2(lado_x, lado_y), Sense::hover());
                        self.miniatura(ctx, c.anexo);
                        if self.desenhar_miniatura(ui, caixa, c.anexo) {
                            self.abrir_visor(ctx, c.anexo, &c.texto, &c.dia, c.tarefa_id);
                        }
                        ui.add_space(8.0 - ui.spacing().item_spacing.x);
                    }
                });
                ui.add_space(8.0);
            }
        }
    }

    /// Salva o .md e as capturas numa pasta "capturas" ao lado, onde você
    /// escolher. É o único ponto em que a tela escreve no disco.
    fn salvar(&mut self, ctx: &egui::Context, sprint: &api::Sprint, _projetos: &[String]) {
        self.salvando = true;
        let nome = format!("sprint-{}-a-{}.md", sprint.de.replace('/', "-"), sprint.ate.replace('/', "-"));
        let markdown = sprint.markdown.clone();
        let capturas: Vec<i64> = sprint.capturas.iter().map(|c| c.anexo).collect();
        em_segundo_plano(&self.canal.0, ctx, move || {
            let Some(arquivo) = rfd::FileDialog::new().set_title("Salvar a sprint").set_file_name(&nome).add_filter("Markdown", &["md"]).save_file() else {
                // Cancelou: nada a avisar.
                return Resposta::Salvo(Err(String::new()));
            };
            let resultado = (|| -> Result<String, String> {
                std::fs::write(&arquivo, markdown).map_err(|e| e.to_string())?;
                if !capturas.is_empty() {
                    let pasta = arquivo.with_file_name("capturas");
                    std::fs::create_dir_all(&pasta).map_err(|e| e.to_string())?;
                    for id in capturas {
                        let png = api::ler_anexo(id)?;
                        std::fs::write(pasta.join(format!("{id}.png")), png).map_err(|e| e.to_string())?;
                    }
                }
                Ok(arquivo.display().to_string())
            })();
            Resposta::Salvo(resultado)
        });
    }
}

/// Item de um bloco da daily: clicável para conferir a tarefa antes de falar.
fn item_resumo(ui: &mut egui::Ui, item: &api::ItemResumo) -> Option<Acao> {
    let p = cores();
    let clicavel = item.tarefa_id != 0 && !item.removida;
    let (rect, resposta) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), if clicavel { Sense::click() } else { Sense::hover() });
    if clicavel && resposta.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(tema::RAIO_ETIQUETA), p.realce);
    }
    let estado = match item.tipo.as_str() {
        "trabalhando" => EstadoVisual::Trabalhando,
        "aguardando" => EstadoVisual::SuaVez,
        t => ponto_do_tipo(t),
    };
    tema::ponto(ui.painter(), pos2(rect.left() + 8.0, rect.center().y), 3.0, estado);
    let cortou = texto_cortado(ui.painter(), pos2(rect.left() + 20.0, rect.center().y), &item.texto, FontId::proportional(13.0), p.texto, rect.width() - 24.0);
    let resposta = if cortou { resposta.on_hover_text(&item.texto) } else { resposta };
    if clicavel && resposta.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        return Some(Acao::AbrirTarefa { tarefa: item.tarefa_id, agente: (item.agente_id != 0).then_some(item.agente_id) });
    }
    None
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn datas_da_tela_viram_datas_do_nucleo() {
        assert_eq!(data_da_tela("25/09/2026").as_deref(), Some("2026-09-25"));
        assert_eq!(data_da_tela(" 01/10/2026 ").as_deref(), Some("2026-10-01"));
        assert!(data_da_tela("2026-09-25").is_none());
        assert!(data_da_tela("1/9/2026").is_none());
        assert!(data_da_tela("aa/bb/cccc").is_none());
    }

    #[test]
    fn miniatura_reduz_a_imagem() {
        let mut png = Vec::new();
        let mut c = png::Encoder::new(&mut png, 400, 300);
        c.set_color(png::ColorType::Rgb);
        c.set_depth(png::BitDepth::Eight);
        let mut e = c.write_header().unwrap();
        e.write_image_data(&vec![200u8; 400 * 300 * 3]).unwrap();
        e.finish().unwrap();
        let img = decodificar(&png, Some(128)).unwrap();
        assert!(img.size[1] <= 128 && img.size[0] > 128);
        assert_eq!(img.pixels[0], Color32::from_rgb(200, 200, 200));
        let inteira = decodificar(&png, None).unwrap();
        assert_eq!(inteira.size, [400, 300]);
    }
}
