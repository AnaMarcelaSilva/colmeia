//! O console de uma conexão: a barra (executar, banco, limite, tempo e o selo
//! de leitura ou escrita), o editor com realce e o painel de baixo (resultado
//! numa grade virtual ou o histórico). Um console por conexão, em memória:
//! trocar de conexão na árvore não perde o rascunho.

use std::ops::Range;
use std::sync::Arc;

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Id, Key, Modifiers, Rect, RichText, Sense, Stroke, StrokeKind, pos2, vec2};

use super::sql;
use crate::api;
use crate::registro;
use crate::tema::{self, Icone, Pilula, cores, forte};

pub const LIMITES: [u32; 4] = [100, 500, 1000, 5000];
pub const TEMPOS: [u32; 5] = [10, 30, 60, 300, 600];
const ALTURA_LINHA_GRADE: f32 = 24.0;
const CABECALHO_GRADE: f32 = 30.0;
const LINHA_EDITOR: f32 = 18.0;

pub fn texto_tempo(s: u32) -> String {
    if s >= 60 { format!("{} min", s / 60) } else { format!("{s} s") }
}

/// O que o console pede à tela de bancos.
#[derive(Clone)]
pub enum Acao {
    Executar,
    Cancelar,
    Mais,
    EditarConexao,
    /// "Trocar senha…" no erro de senha recusada (a instrução roda de novo
    /// depois que a senha nova for aceita).
    TrocarSenha,
    LerHistorico,
    LimparHistorico,
    /// "A seleção tem 3 instruções" e afins: um aviso só no painel.
    Avisar(String),
}

/// O que está no painel de baixo.
pub enum Painel {
    Nada,
    Resultado {
        r: api::Resultado,
        ficha: String,
        banco: String,
        /// "loja.pedidos" na prévia de uma tabela.
        previa: Option<String>,
        /// A execução aberta fechou (parada demais): carregar mais não dá.
        fechada: bool,
    },
    Falhou(api::FalhaBanco),
    /// Informação (não é problema): "A seleção tem 3 instruções…".
    Info(String),
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Selecao {
    Nenhuma,
    Celula(usize, usize),
    Linha(usize),
    Tudo,
}

pub struct Rodando {
    pub ficha: String,
    pub desde: f64,
    /// Carregando mais linhas (não uma execução nova).
    pub mais: bool,
}

pub struct Console {
    pub texto: String,
    /// Banco escolhido na barra ("" é o padrão da conexão).
    pub banco: String,
    pub limite: u32,
    pub tempo_s: u32,
    pub executando: Option<Rodando>,
    pub painel: Painel,
    pub aba_historico: bool,
    pub historico: Option<Result<Vec<api::ConsultaFeita>, String>>,
    pub selecao: Selecao,
    pub larguras: Vec<f32>,
    /// Casas decimais por coluna de números (para alinhar os pontos).
    casas: Vec<usize>,
    /// A janela de leitura de uma célula longa.
    pub celula_aberta: Option<(usize, usize)>,
    /// O trecho que acabou de ser executado (em bytes) e até quando fica destacado.
    pub marcado: Option<(Range<usize>, f64)>,
    pub focar: bool,
    /// O editor tinha o foco no fim do quadro anterior: o egui tira o foco
    /// do campo ao receber o Esc antes de o console ver a tecla.
    tinha_foco: bool,
    /// Onde pôr o cursor no próximo quadro (em caracteres).
    pub cursor_para: Option<usize>,
    cache: Option<(u64, Arc<egui::Galley>)>,
    contador: u64,
    /// O erro no painel veio da prévia desta tabela (banco, esquema, tabela).
    pub previa_falhou: Option<(String, String, String)>,
}

impl Console {
    pub fn novo(banco: &str) -> Console {
        Console {
            texto: String::new(),
            banco: banco.to_string(),
            limite: 500,
            tempo_s: 30,
            executando: None,
            painel: Painel::Nada,
            aba_historico: false,
            historico: None,
            selecao: Selecao::Nenhuma,
            larguras: Vec::new(),
            casas: Vec::new(),
            celula_aberta: None,
            marcado: None,
            focar: false,
            tinha_foco: false,
            cursor_para: None,
            cache: None,
            contador: 0,
            previa_falhou: None,
        }
    }

    /// Uma ficha nova para a execução (o núcleo cancela e carrega mais por ela).
    pub fn nova_ficha(&mut self, conexao: i64) -> String {
        self.contador += 1;
        let agora = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        format!("tela-{conexao}-{}-{:x}", self.contador, agora)
    }

    /// Põe o texto abaixo da linha do cursor, depois de uma linha em branco e
    /// terminado em `;`, com o cursor nele (nunca substitui o rascunho). Assim
    /// o Ctrl+Enter logo depois executa só ele.
    pub fn inserir(&mut self, ctx: &egui::Context, texto: &str) {
        let id = id_editor();
        let cursor = egui::TextEdit::load_state(ctx, id).and_then(|s| s.cursor.char_range()).map_or(self.texto.chars().count(), |r| r.primary.index.0);
        let (fim_da_linha, inserido) = texto_inserido(&self.texto, sql::byte_do_caractere(&self.texto, cursor), texto);
        self.texto.insert_str(fim_da_linha, &inserido);
        // O cursor fica antes do ";", dentro da instrução.
        self.cursor_para = Some(sql::caractere_do_byte(&self.texto, fim_da_linha + inserido.len() - 1));
        self.focar = true;
    }

    pub fn resultado_novo(&mut self, r: api::Resultado, ficha: String, banco: String, previa: Option<String>) {
        self.larguras = medir_larguras(&r);
        self.casas = casas_decimais(&r);
        self.selecao = Selecao::Nenhuma;
        self.celula_aberta = None;
        self.aba_historico = false;
        self.painel = Painel::Resultado { r, ficha, banco, previa, fechada: false };
    }

    pub fn mais_linhas(&mut self, mais: api::Resultado) {
        if let Painel::Resultado { r, .. } = &mut self.painel {
            r.linhas.extend(mais.linhas);
            r.mais = mais.mais;
            r.ms += mais.ms;
            self.casas = casas_decimais(r);
        }
    }

    /// A barra do console. Devolve as ações.
    #[allow(clippy::too_many_arguments)]
    pub fn barra(&mut self, ui: &mut egui::Ui, conexao: &api::ConexaoBanco, bancos: &[String], agora: f64, acoes: &mut Vec<Acao>) {
        let p = cores();
        let estreita = ui.available_width() < 820.0;
        ui.horizontal(|ui| {
            ui.set_height(34.0);
            ui.spacing_mut().item_spacing.x = 8.0;
            if let Some(r) = &self.executando {
                if tema::botao_alerta(ui, "Cancelar").clicked() {
                    acoes.push(Acao::Cancelar);
                }
                if !estreita {
                    tema::tecla(ui, "Esc");
                }
                let s = (agora - r.desde).max(0.0) as u64;
                ui.label(RichText::new(format!("Executando · {s} s")).color(p.suave).size(13.0));
            } else {
                let pode = !self.texto.trim().is_empty();
                if tema::botao_principal(ui, "Executar", pode).clicked() {
                    acoes.push(Acao::Executar);
                }
                if !estreita {
                    tema::tecla(ui, "Ctrl+Enter");
                }
            }
            ui.add_space(4.0);
            // A conexão (onde o Ctrl+Enter vai rodar): o tipo, o nome e o selo
            // Só leitura/Escrita ao lado. Dois "loja" em servidores diferentes
            // não se confundem.
            tema::etiqueta_ui(ui, sigla_do_tipo(&conexao.tipo), tema::fonte_etiqueta(), p.suave).on_hover_text(nome_do_tipo(&conexao.tipo));
            let g = tema::cortar(ui.painter(), &conexao.nome, egui::TextFormat::simple(forte(13.0), p.texto), if estreita { 120.0 } else { 220.0 }, 1, true);
            let (r, resposta) = ui.allocate_exact_size(g.size(), Sense::hover());
            ui.painter().galley(r.min, g, p.texto);
            resposta.on_hover_text(format!("Conexão {}", conexao.nome));
            let pilula = if conexao.escrita {
                Pilula { texto: "Escrita", cor: p.alerta, cheio: true, ponto: true, grande: false }
            } else {
                Pilula::neutra("Só leitura")
            };
            let largura = pilula.largura(ui.painter());
            let (r, resposta) = ui.allocate_exact_size(vec2(largura, 22.0), Sense::hover());
            pilula.pintar(ui.painter(), r.min);
            resposta.on_hover_text(if conexao.escrita {
                "Alterações permitidas: cada uma pede confirmação"
            } else {
                "Só leitura: alterações são recusadas pelo núcleo"
            });
            if conexao.tipo != "sqlite" {
                let valor = if self.banco.is_empty() { if conexao.banco.is_empty() { "padrão" } else { conexao.banco.as_str() } } else { self.banco.as_str() };
                let resposta = tema::chip(ui, "Banco", valor, false);
                egui::Popup::menu(&resposta).show(|ui| {
                    ui.set_min_width(220.0);
                    egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                        if tema::opcao_menu(ui, "Padrão da conexão", self.banco.is_empty()) {
                            self.banco.clear();
                            ui.close();
                        }
                        for b in bancos {
                            if tema::opcao_menu(ui, b, self.banco == *b) {
                                self.banco = b.clone();
                                ui.close();
                            }
                        }
                    });
                });
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let padrao = self.limite == 500 && self.tempo_s == 30;
                if estreita {
                    let valor = format!("{} · {}", self.limite, texto_tempo(self.tempo_s));
                    let resposta = tema::chip(ui, "Opções", &valor, !padrao);
                    egui::Popup::menu(&resposta).show(|ui| {
                        ui.set_min_width(200.0);
                        self.opcoes_limite(ui);
                        ui.separator();
                        self.opcoes_tempo(ui);
                    });
                } else {
                    let resposta = tema::chip(ui, "Tempo", &texto_tempo(self.tempo_s), self.tempo_s != 30);
                    egui::Popup::menu(&resposta).show(|ui| {
                        ui.set_min_width(180.0);
                        self.opcoes_tempo(ui);
                    });
                    let resposta = tema::chip(ui, "Limite", &self.limite.to_string(), self.limite != 500);
                    egui::Popup::menu(&resposta).show(|ui| {
                        ui.set_min_width(180.0);
                        self.opcoes_limite(ui);
                    });
                }
            });
        });
    }

    fn opcoes_limite(&mut self, ui: &mut egui::Ui) {
        for l in LIMITES {
            if tema::opcao_menu(ui, &format!("{l} linhas"), self.limite == l) {
                self.limite = l;
                ui.close();
            }
        }
    }

    fn opcoes_tempo(&mut self, ui: &mut egui::Ui) {
        for t in TEMPOS {
            if tema::opcao_menu(ui, &format!("Tempo-limite {}", texto_tempo(t)), self.tempo_s == t) {
                self.tempo_s = t;
                ui.close();
            }
        }
    }

    /// O editor: Ctrl+Enter executa a seleção ou a instrução sob o cursor;
    /// Esc cancela a execução. O realce só refaz o layout quando o texto muda.
    pub fn editor(&mut self, ui: &mut egui::Ui, area: Rect, dialeto: &str, agora: f64, acoes: &mut Vec<Acao>) {
        let p = cores();
        let id = id_editor();
        let ctx = ui.ctx().clone();
        // O foco é pedido uma vez (depois de o clique que pediu soltar o botão).
        if self.focar && ui.input(|i| i.pointer.any_released()) {
            ctx.request_repaint();
        } else if std::mem::take(&mut self.focar) {
            ctx.memory_mut(|m| m.request_focus(id));
        }
        if let Some(c) = self.cursor_para.take() {
            let mut estado = egui::TextEdit::load_state(&ctx, id).unwrap_or_default();
            estado.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(c))));
            estado.store(&ctx, id);
        }
        let com_foco = ctx.memory(|m| m.has_focus(id));
        if com_foco && ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Enter)) {
            if self.executando.is_some() {
                acoes.push(Acao::Avisar("Já há uma consulta rodando".into()));
            } else {
                acoes.push(Acao::Executar);
            }
        }
        if self.executando.is_some() && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            acoes.push(Acao::Cancelar);
            // O Esc também tira o foco do campo (o egui já tirou neste quadro,
            // por isso vale o do quadro anterior): ele volta para continuar escrevendo.
            if com_foco || self.tinha_foco {
                self.focar = true;
                ctx.request_repaint();
            }
        }
        // O destaque do trecho executado some sozinho (um redesenho só, no fim dele).
        let marcado = match &self.marcado {
            Some((r, ate)) if agora < *ate => {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(ate - agora));
                Some(r.clone())
            }
            _ => {
                self.marcado = None;
                None
            }
        };
        let fundo = tema::fundo_campo(p, false);
        let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(area));
        egui::Frame::new()
            .fill(fundo)
            .stroke(if com_foco { Stroke::new(1.5, p.destaque) } else { Stroke::new(1.0, p.borda) })
            .corner_radius(CornerRadius::same(tema::RAIO_CONTROLE))
            .inner_margin(egui::Margin::symmetric(10, 8))
            .show(&mut filho, |ui| {
                ui.set_min_size(area.size() - vec2(20.0, 16.0));
                ui.set_max_size(area.size() - vec2(20.0, 16.0));
                let cor_marca = tema::misturar(fundo, p.destaque, if tema::claro() { 0.14 } else { 0.18 });
                // A galeria guardada aponta para o atlas das letras: refeito o atlas
                // (troca de tema, de escala), a galeria é montada de novo.
                let geracao = crate::lousa::desenho::geracao_das_fontes(ui.ctx());
                let cache = &mut self.cache;
                let mut layouter = |ui: &egui::Ui, texto: &dyn egui::TextBuffer, largura: f32| {
                    let texto = texto.as_str();
                    let chave = {
                        use std::hash::{Hash, Hasher};
                        let mut h = std::collections::hash_map::DefaultHasher::new();
                        texto.hash(&mut h);
                        largura.to_bits().hash(&mut h);
                        marcado.hash(&mut h);
                        tema::claro().hash(&mut h);
                        p.texto.hash(&mut h);
                        dialeto.hash(&mut h);
                        geracao.hash(&mut h);
                        h.finish()
                    };
                    if let Some((c, g)) = cache.as_ref()
                        && *c == chave
                    {
                        return g.clone();
                    }
                    let mut job = sql::realce(dialeto, texto, FontId::monospace(13.0), p, marcado.clone().map(|r| (r, cor_marca)));
                    job.wrap.max_width = largura;
                    let g = ui.fonts_mut(|f| f.layout_job(job));
                    *cache = Some((chave, g.clone()));
                    g
                };
                let visivel = ui.available_size();
                egui::ScrollArea::both().id_salt("rolagem-editor").auto_shrink(false).show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        let (calha, _) = ui.allocate_exact_size(vec2(36.0, 10.0), Sense::hover());
                        // O campo ocupa a área toda: clicar em qualquer ponto do editor põe o cursor nele.
                        let saida = egui::TextEdit::multiline(&mut self.texto)
                            .min_size(vec2((visivel.x - 44.0).max(40.0), (visivel.y - 4.0).max(LINHA_EDITOR)))
                            .id(id)
                            .frame(egui::Frame::NONE)
                            .font(FontId::monospace(13.0))
                            .lock_focus(true)
                            .desired_width(f32::INFINITY)
                            .desired_rows(4)
                            .hint_text(RichText::new("-- Escreva SQL aqui. Ctrl+Enter executa a instrução sob o cursor.").monospace().color(p.suave))
                            .layouter(&mut layouter)
                            .show(ui);
                        self.tinha_foco = saida.response.has_focus();
                        // Números das linhas: o da linha do cursor em `texto`.
                        let linha_cursor = saida.cursor_range.map(|r| {
                            let b = sql::byte_do_caractere(&self.texto, r.primary.index.0);
                            self.texto[..b].matches('\n').count()
                        });
                        let mut numero = 0;
                        let mut comeca = true;
                        if self.texto.is_empty() {
                            // Vazio, o campo mostra o texto de exemplo: o "1" fica na primeira linha.
                            let y = saida.galley_pos.y + LINHA_EDITOR / 2.0;
                            ui.painter().text(pos2(calha.right(), y), Align2::RIGHT_CENTER, "1", FontId::monospace(11.5), p.suave);
                            comeca = false;
                        }
                        for row in &saida.galley.rows {
                            if comeca {
                                let y = saida.galley_pos.y + row.rect().center().y;
                                let cor = if linha_cursor == Some(numero) { p.texto } else { p.suave };
                                ui.painter().text(pos2(calha.right(), y), Align2::RIGHT_CENTER, (numero + 1).to_string(), FontId::monospace(11.5), cor);
                                numero += 1;
                            }
                            comeca = row.ends_with_newline;
                        }
                    });
                });
            });
    }

    /// O trecho a executar: a seleção ou a instrução sob o cursor. Erro com
    /// o texto para o painel quando não há o que executar.
    pub fn trecho(&self, ctx: &egui::Context, dialeto: &str) -> Result<(Range<usize>, String), String> {
        let id = id_editor();
        let faixa = egui::TextEdit::load_state(ctx, id).and_then(|s| s.cursor.char_range());
        let (a, b) = match faixa {
            Some(r) => {
                let s = r.as_sorted_char_range();
                (sql::byte_do_caractere(&self.texto, s.start.0), sql::byte_do_caractere(&self.texto, s.end.0))
            }
            None => (self.texto.len(), self.texto.len()),
        };
        if a != b {
            let selecionado = &self.texto[a..b];
            let n = sql::dividir(dialeto, selecionado).len();
            if n > 1 {
                return Err(format!("A seleção tem {n} instruções; a Colmeia executa uma por vez. Ponha o cursor na que quer."));
            }
            if n == 0 {
                return Err("A seleção não tem instrução.".into());
            }
            let t = &sql::dividir(dialeto, selecionado)[0];
            return Ok((a + t.start..a + t.end, selecionado[t.clone()].to_string()));
        }
        match sql::sob_cursor(dialeto, &self.texto, a) {
            Some(r) => Ok((r.clone(), self.texto[r].to_string())),
            None => Err("Não há instrução para executar.".into()),
        }
    }

    /// O painel de baixo: as abas, o corpo e o rodapé.
    #[allow(clippy::too_many_arguments)]
    pub fn painel(&mut self, ui: &mut egui::Ui, area: Rect, dialeto: &str, conexao_padrao: &str, agentes: &dyn Fn(i64) -> String, acoes: &mut Vec<Acao>) {
        let p = cores();
        let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(area));
        let ui = &mut filho;
        ui.painter().rect(area, CornerRadius::same(tema::RAIO_SUPERFICIE), p.superficie_alta, Stroke::new(1.0, p.borda), StrokeKind::Inside);
        // Cabeçalho, 44 px.
        let cabecalho = Rect::from_min_size(area.min, vec2(area.width(), 44.0));
        let mut topo =
            ui.new_child(egui::UiBuilder::new().max_rect(cabecalho.shrink2(vec2(12.0, 5.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
        let n_hist = match &self.historico {
            Some(Ok(h)) => format!("Histórico ({})", h.len()),
            _ => "Histórico".into(),
        };
        let atual = usize::from(self.aba_historico);
        if let Some(i) = tema::segmentado(&mut topo, &["Resultado", &n_hist], atual) {
            self.aba_historico = i == 1;
            if self.aba_historico {
                acoes.push(Acao::LerHistorico);
            }
        }
        if !self.aba_historico
            && let Painel::Resultado { previa: Some(nome), .. } = &self.painel
        {
            topo.add_space(8.0);
            topo.label(tema::texto_forte(format!("Prévia · {nome}"), 13.0).color(p.texto));
            topo.label(RichText::new(" · 100 primeiras linhas").color(p.suave).size(13.0));
        }
        if self.aba_historico {
            topo.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let tem = matches!(&self.historico, Some(Ok(h)) if !h.is_empty());
                if tema::botao_secundario_com(ui, "Limpar histórico", tem).clicked() {
                    acoes.push(Acao::LimparHistorico);
                }
            });
        }
        let com_rodape = !self.aba_historico && matches!(self.painel, Painel::Resultado { .. });
        let corpo =
            Rect::from_min_max(pos2(area.left() + 1.0, cabecalho.bottom()), pos2(area.right() - 1.0, area.bottom() - if com_rodape { 36.0 } else { 1.0 }));
        if self.aba_historico {
            self.lista_historico(ui, corpo, dialeto, agentes);
            return;
        }
        match &self.painel {
            Painel::Nada => estado_vazio(ui, corpo),
            Painel::Info(texto) => {
                bloco(ui, corpo, texto, &[], p.suave, true, None, false);
            }
            Painel::Falhou(f) => {
                let (cor, botoes): (_, &[(&str, Acao)]) = if f.somente_leitura {
                    (p.alerta, &[("Editar conexão", Acao::EditarConexao)])
                } else if f.senha_recusada {
                    (p.erro, &[("Trocar senha…", Acao::TrocarSenha), ("Editar conexão", Acao::EditarConexao)])
                } else if f.tempo_esgotado || f.cancelada {
                    (p.alerta, &[])
                } else {
                    (p.erro, &[])
                };
                let texto = if f.detalhe.is_empty() { f.erro.clone() } else { format!("{}\n{}", f.erro, f.detalhe) };
                let linha = (f.linha > 0).then(|| format!("Linha {}", f.linha));
                // Sem frase da Colmeia (só a mensagem do servidor), o texto todo é do driver: mono.
                let do_driver = !f.somente_leitura && !f.tempo_esgotado && !f.cancelada && f.detalhe.is_empty() && (f.linha > 0 || f.do_servidor);
                let rotulos: Vec<&str> = botoes.iter().map(|(r, _)| *r).collect();
                if let Some(i) = bloco(ui, corpo, &texto, &rotulos, cor, false, linha.as_deref(), do_driver) {
                    acoes.push(botoes[i].1.clone());
                }
            }
            Painel::Resultado { r, .. } if r.afetadas.is_some() => {
                // Uma alteração não tem grade: o rodapé diz o que foi gravado.
                let texto = format!("{} executado.", r.verbo);
                ui.painter().text(corpo.center(), Align2::CENTER_CENTER, texto, FontId::proportional(13.0), p.suave);
                let rodape = Rect::from_min_max(pos2(area.left(), corpo.bottom()), area.max);
                self.rodape(ui, rodape, conexao_padrao, acoes);
            }
            Painel::Resultado { .. } => {
                self.grade(ui, corpo);
                let rodape = Rect::from_min_max(pos2(area.left(), corpo.bottom()), area.max);
                self.rodape(ui, rodape, conexao_padrao, acoes);
            }
        }
    }

    fn rodape(&mut self, ui: &mut egui::Ui, rodape: Rect, conexao_padrao: &str, acoes: &mut Vec<Acao>) {
        let p = cores();
        let Painel::Resultado { r, banco, fechada, .. } = &self.painel else { return };
        ui.painter().line_segment([rodape.left_top() + vec2(1.0, 0.0), rodape.right_top() - vec2(1.0, 0.0)], Stroke::new(1.0, p.borda));
        let banco = if banco.is_empty() { conexao_padrao } else { banco.as_str() };
        let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(rodape.shrink2(vec2(12.0, 2.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
        if let Some(n) = r.afetadas {
            // Depois de uma alteração: já está gravado.
            let (rr, _) = filho.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
            filho.painter().circle_filled(rr.center(), 3.5, p.ok);
            let linhas = if n == 1 { "1 linha alterada".to_string() } else { format!("{n} linhas alteradas") };
            filho.label(RichText::new(format!("{} · {linhas} · {} ms · já gravado no banco", r.verbo, r.ms)).color(p.texto).size(12.5));
            return;
        }
        let mut partes = vec![if r.linhas.len() == 1 { "1 linha".to_string() } else { format!("{} linhas", r.linhas.len()) }];
        if r.mais {
            partes.push("há mais".into());
        }
        partes.push(format!("{} ms", r.ms));
        if !banco.is_empty() {
            partes.push(format!("banco {banco}"));
        }
        filho.label(RichText::new(partes.join(" · ")).color(p.suave).size(12.5));
        if r.mais {
            let limite = self.limite;
            let carregando = self.executando.as_ref().is_some_and(|e| e.mais);
            filho.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if *fechada {
                    if tema::botao_secundario_com(ui, "Executar de novo para ver mais", true).clicked() {
                        acoes.push(Acao::Executar);
                    }
                } else if tema::botao_secundario_com(ui, &format!("Carregar mais {limite}"), !carregando).clicked() {
                    acoes.push(Acao::Mais);
                }
            });
        }
    }

    /// A grade: cabeçalho fixo, calha de números, só as linhas e colunas
    /// visíveis desenhadas. Clique escolhe a célula, a calha escolhe a linha,
    /// Ctrl+C copia (TSV) e Ctrl+A escolhe tudo. O cabeçalho fica fora da
    /// área que rola (a barra vertical começa abaixo dele) e acompanha a
    /// rolagem horizontal.
    fn grade(&mut self, ui: &mut egui::Ui, corpo: Rect) {
        let p = cores();
        let Painel::Resultado { r, ficha, .. } = &self.painel else { return };
        if self.larguras.len() != r.colunas.len() {
            self.larguras = medir_larguras(r);
        }
        if self.casas.len() != r.colunas.len() {
            self.casas = casas_decimais(r);
        }
        let n = r.linhas.len();
        let pintor = ui.painter().clone();
        let calha = (pintor.layout_no_wrap("10000".into(), FontId::monospace(11.5), p.suave).size().x + 16.0).max(44.0);
        // A barra de rolagem flutua sobre o conteúdo: sobra espaço para ela no
        // fim, para a última linha e a última coluna não ficarem por baixo.
        let barra = ui.spacing().scroll.bar_width + ui.spacing().scroll.bar_outer_margin + 4.0;
        let largura_total = calha + self.larguras.iter().sum::<f32>() + 8.0 + barra;
        let altura_total = n.max(1) as f32 * ALTURA_LINHA_GRADE + 8.0 + barra;
        let cab = Rect::from_min_size(corpo.min, vec2(corpo.width(), CABECALHO_GRADE));
        // 1 px acima do fio do rodapé.
        let area_linhas = Rect::from_min_max(pos2(corpo.left(), cab.bottom()), pos2(corpo.right(), corpo.bottom() - 1.0));
        let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(area_linhas));
        filho.set_clip_rect(area_linhas.intersect(ui.clip_rect()));
        let mut clique: Option<Selecao> = None;
        let mut duplo: Option<(usize, usize)> = None;
        let mut menu: Option<(&'static str, usize, usize)> = None;
        let mut arrasto: Option<(usize, f32)> = None;
        let larguras = &self.larguras;
        let casas = &self.casas;
        let selecao = self.selecao;
        let saida = egui::ScrollArea::both().id_salt(("grade-resultado", ficha)).auto_shrink(false).show_viewport(&mut filho, |ui, janela| {
            let (rect, _) = ui.allocate_exact_size(vec2(largura_total.max(janela.width()), altura_total.max(janela.height())), Sense::hover());
            let origem = rect.min;
            let pintor = ui.painter().clone();
            let esquerda_visivel = origem.x + janela.min.x;
            // As linhas visíveis.
            let primeira = (janela.min.y / ALTURA_LINHA_GRADE).floor().max(0.0) as usize;
            let ultima = ((janela.max.y / ALTURA_LINHA_GRADE).ceil() as usize + 1).min(n);
            let colunas = colunas_visiveis(larguras, origem.x + calha, esquerda_visivel + calha, origem.x + janela.max.x);
            let mono = FontId::monospace(12.5);
            for l in primeira..ultima {
                let y = origem.y + l as f32 * ALTURA_LINHA_GRADE;
                let faixa = Rect::from_min_size(pos2(esquerda_visivel, y), vec2(janela.width(), ALTURA_LINHA_GRADE));
                let linha_escolhida = selecao == Selecao::Linha(l) || selecao == Selecao::Tudo;
                let resposta_linha = ui.interact(faixa, Id::new(("linha-grade", l)), Sense::hover());
                if linha_escolhida {
                    pintor.rect_filled(faixa, CornerRadius::ZERO, tema::fundo_escolhido());
                } else if resposta_linha.hovered() {
                    pintor.rect_filled(faixa, CornerRadius::ZERO, p.realce);
                }
                pintor.line_segment(
                    [pos2(faixa.left(), y + ALTURA_LINHA_GRADE - 0.5), pos2(faixa.right(), y + ALTURA_LINHA_GRADE - 0.5)],
                    Stroke::new(1.0, p.borda.gamma_multiply(0.6)),
                );
                for &(c, x_col, largura_col) in &colunas {
                    let celula = Rect::from_min_size(pos2(x_col, y), vec2(largura_col, ALTURA_LINHA_GRADE));
                    let escolhida = selecao == Selecao::Celula(l, c);
                    if escolhida {
                        pintor.rect(celula, CornerRadius::ZERO, tema::fundo_escolhido(), Stroke::new(1.5, p.destaque), StrokeKind::Inside);
                    }
                    let valor = r.linhas[l].get(c).cloned().flatten();
                    let realcada = escolhida || linha_escolhida;
                    let apagada = if realcada { p.texto } else { p.suave };
                    let numero = r.colunas[c].numero;
                    let (texto, cor, italico) = match &valor {
                        None => ("NULL".to_string(), apagada, true),
                        Some(v) if v.starts_with("<binário ") => (v.clone(), apagada, true),
                        Some(v) if numero => (alinhar_decimal(v, casas.get(c).copied().unwrap_or(0)), p.texto, false),
                        Some(v) => {
                            let (t, controle) = texto_da_celula(v);
                            (t, if controle { apagada } else { p.texto }, controle)
                        }
                    };
                    let formato = egui::TextFormat { font_id: mono.clone(), color: cor, italics: italico, ..Default::default() };
                    let g = tema::cortar(&pintor, &texto, formato, largura_col - 16.0, 1, true);
                    let x_texto = if numero && valor.is_some() { celula.right() - 8.0 - g.size().x } else { celula.left() + 8.0 };
                    pintor.galley(pos2(x_texto, celula.center().y - g.size().y / 2.0), g, cor);
                    let resposta = ui.interact(celula, Id::new(("celula", l, c)), Sense::click());
                    if resposta.double_clicked() {
                        duplo = Some((l, c));
                    } else if resposta.clicked() {
                        clique = Some(Selecao::Celula(l, c));
                    }
                    resposta.context_menu(|ui| {
                        ui.set_min_width(200.0);
                        for (rotulo, chave) in [("Copiar valor", "valor"), ("Copiar linha", "linha"), ("Copiar coluna", "coluna"), ("Copiar tudo", "tudo")] {
                            if tema::opcao_menu_com(ui, rotulo, None, true) {
                                menu = Some((chave, l, c));
                                ui.close();
                            }
                        }
                    });
                }
                // A calha, presa à esquerda.
                let gutter = Rect::from_min_size(pos2(esquerda_visivel, y), vec2(calha, ALTURA_LINHA_GRADE));
                pintor.rect_filled(gutter, CornerRadius::ZERO, p.superficie);
                if linha_escolhida {
                    pintor.rect_filled(
                        Rect::from_min_size(gutter.min + vec2(0.0, 2.0), vec2(2.0, ALTURA_LINHA_GRADE - 4.0)),
                        CornerRadius::same(1),
                        p.destaque,
                    );
                }
                let cor = if linha_escolhida { p.texto } else { p.suave };
                pintor.text(pos2(gutter.right() - 8.0, gutter.center().y), Align2::RIGHT_CENTER, (l + 1).to_string(), FontId::monospace(11.5), cor);
                if ui.interact(gutter, Id::new(("calha", l)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    clique = Some(Selecao::Linha(l));
                }
            }
            if n == 0 {
                let texto = format!("Nenhuma linha · {} ms", r.ms);
                pintor.text(
                    pos2(esquerda_visivel + janela.width() / 2.0, origem.y + janela.min.y + 24.0),
                    Align2::CENTER_TOP,
                    texto,
                    FontId::proportional(13.0),
                    p.suave,
                );
            }
        });
        // Cabeçalho, fora da rolagem vertical, deslocado pela horizontal.
        let deslocado = saida.state.offset.x;
        let pintor = ui.painter().with_clip_rect(cab.intersect(ui.clip_rect()));
        pintor.rect_filled(cab, CornerRadius::ZERO, p.superficie);
        pintor.line_segment([cab.left_bottom() - vec2(0.0, 0.5), cab.right_bottom() - vec2(0.0, 0.5)], Stroke::new(1.0, p.borda));
        let x0 = cab.left() + calha - deslocado;
        for (c, x_col, largura_col) in colunas_visiveis(&self.larguras, x0, cab.left() + calha, cab.right()) {
            let celula = Rect::from_min_size(pos2(x_col, cab.top()), vec2(largura_col, CABECALHO_GRADE));
            let col = &r.colunas[c];
            let g_nome = pintor.layout_no_wrap(col.nome.clone(), forte(12.5), p.texto);
            let g_tipo = pintor.layout_no_wrap(col.tipo.clone(), FontId::proportional(11.0), p.suave);
            let cabe_tipo = g_nome.size().x + 6.0 + g_tipo.size().x <= celula.width() - 16.0;
            let g_nome = tema::cortar(&pintor, &col.nome, egui::TextFormat::simple(forte(12.5), p.texto), celula.width() - 16.0, 1, true);
            let x_nome = celula.left() + 8.0;
            pintor.galley(pos2(x_nome, celula.center().y - g_nome.size().y / 2.0), g_nome.clone(), p.texto);
            if cabe_tipo && !col.tipo.is_empty() {
                pintor.galley(pos2(x_nome + g_nome.size().x + 6.0, celula.center().y - g_tipo.size().y / 2.0), g_tipo, p.suave);
            }
            // Divisória da coluna: arrastar ajusta a largura.
            let divisa = Rect::from_center_size(pos2(celula.right(), celula.center().y), vec2(8.0, CABECALHO_GRADE)).intersect(cab);
            if divisa.width() <= 0.0 {
                continue;
            }
            let resposta = ui.interact(divisa, Id::new(("divisa-coluna", c)), Sense::drag()).on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
            let ativa = resposta.hovered() || resposta.dragged();
            let traco = if ativa { Stroke::new(2.0, p.destaque) } else { Stroke::new(1.0, p.borda) };
            pintor.line_segment([pos2(celula.right(), celula.center().y - 7.0), pos2(celula.right(), celula.center().y + 7.0)], traco);
            if resposta.dragged_by(egui::PointerButton::Primary) {
                arrasto = Some((c, resposta.drag_delta().x));
            }
        }
        pintor.rect_filled(Rect::from_min_size(cab.min, vec2(calha, CABECALHO_GRADE)), CornerRadius::ZERO, p.superficie);
        if let Some((c, d)) = arrasto {
            self.larguras[c] = (self.larguras[c] + d).clamp(48.0, 1200.0);
        }
        if let Some(s) = clique {
            self.selecao = s;
        }
        if let Some(d) = duplo {
            self.celula_aberta = Some(d);
            self.selecao = Selecao::Celula(d.0, d.1);
        }
        // Copiar: o Ctrl+C vira Event::Copy (fora de um campo de texto em foco).
        let editor_com_foco = ui.memory(|m| m.focused().is_some_and(|f| f == id_editor() || f == Id::new("filtro-arvore-banco")));
        if !editor_com_foco && self.selecao != Selecao::Nenhuma {
            let copiar = ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Copy)));
            if copiar && let Some(t) = self.texto_da_selecao(self.selecao) {
                ui.ctx().copy_text(t);
            }
        }
        if !editor_com_foco && ui.rect_contains_pointer(corpo) && ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::A)) {
            self.selecao = Selecao::Tudo;
        }
        if let Some((o_que, l, c)) = menu {
            let sel = match o_que {
                "valor" => Selecao::Celula(l, c),
                "linha" => Selecao::Linha(l),
                "tudo" => Selecao::Tudo,
                _ => Selecao::Nenhuma,
            };
            let texto = if o_que == "coluna" { self.texto_da_coluna(c) } else { self.texto_da_selecao(sel) };
            if let Some(t) = texto {
                ui.ctx().copy_text(t);
            }
        }
        self.janela_da_celula(ui.ctx());
    }

    fn texto_da_selecao(&self, sel: Selecao) -> Option<String> {
        let Painel::Resultado { r, .. } = &self.painel else { return None };
        let celula = |v: &Option<String>| v.clone().unwrap_or_else(|| "NULL".into()).replace(['\t', '\n'], " ");
        let linha = |l: &Vec<Option<String>>| l.iter().map(celula).collect::<Vec<_>>().join("\t");
        match sel {
            Selecao::Nenhuma => None,
            Selecao::Celula(l, c) => r.linhas.get(l).and_then(|x| x.get(c)).map(|v| v.clone().unwrap_or_else(|| "NULL".into())),
            Selecao::Linha(l) => r.linhas.get(l).map(linha),
            Selecao::Tudo => {
                let mut t = r.colunas.iter().map(|c| c.nome.clone()).collect::<Vec<_>>().join("\t");
                for l in &r.linhas {
                    t.push('\n');
                    t.push_str(&linha(l));
                }
                Some(t)
            }
        }
    }

    fn texto_da_coluna(&self, c: usize) -> Option<String> {
        let Painel::Resultado { r, .. } = &self.painel else { return None };
        let mut t = r.colunas.get(c)?.nome.clone();
        for l in &r.linhas {
            t.push('\n');
            t.push_str(&l.get(c).cloned().flatten().unwrap_or_else(|| "NULL".into()));
        }
        Some(t)
    }

    /// A janela de leitura de uma célula (clique duplo).
    fn janela_da_celula(&mut self, ctx: &egui::Context) {
        let Some((l, c)) = self.celula_aberta else { return };
        let Painel::Resultado { r, .. } = &self.painel else { return };
        let (Some(coluna), Some(valor)) = (r.colunas.get(c), r.linhas.get(l).and_then(|x| x.get(c))) else {
            self.celula_aberta = None;
            return;
        };
        let mut texto = valor.clone().unwrap_or_else(|| "NULL".into());
        let titulo = coluna.nome.clone();
        let explicacao = format!("Linha {} · {}", l + 1, coluna.tipo);
        let mut fechar = false;
        let modal = egui::Modal::new(Id::new("janela-celula")).frame(tema::moldura_janela()).show(ctx, |ui| {
            ui.set_width(560.0);
            tema::cabecalho(ui, &titulo, &explicacao);
            ui.add_space(12.0);
            tema::campo_multilinha_com(ui, &mut texto, 6, 360.0, Id::new("texto-celula"), true, FontId::monospace(12.5));
            ui.add_space(20.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if tema::botao_principal(ui, "Fechar", true).clicked() {
                        fechar = true;
                    }
                    ui.add_space(8.0);
                    if tema::botao_secundario(ui, "Copiar").clicked() {
                        ui.ctx().copy_text(texto.clone());
                    }
                });
            });
        });
        if fechar || modal.should_close() {
            self.celula_aberta = None;
        }
    }

    fn lista_historico(&mut self, ui: &mut egui::Ui, corpo: Rect, dialeto: &str, agentes: &dyn Fn(i64) -> String) {
        let p = cores();
        let lista = match &self.historico {
            None => {
                ui.painter().text(corpo.center(), Align2::CENTER_CENTER, "Carregando…", FontId::proportional(13.0), p.suave);
                return;
            }
            Some(Err(e)) => {
                bloco(ui, corpo, e, &[], p.erro, false, None, false);
                return;
            }
            Some(Ok(h)) if h.is_empty() => {
                ui.painter().text(corpo.center(), Align2::CENTER_CENTER, "Nenhuma consulta nesta conexão ainda.", FontId::proportional(13.0), p.suave);
                return;
            }
            Some(Ok(h)) => h.clone(),
        };
        let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(corpo.shrink2(vec2(12.0, 4.0))));
        let mut inserir = None;
        egui::ScrollArea::vertical().id_salt("historico-banco").auto_shrink(false).show_rows(&mut filho, 48.0, lista.len(), |ui, faixa| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for i in faixa {
                let c = &lista[i];
                let largura = ui.available_width() - registro::MARGEM_ROLAGEM;
                let (rect, resposta) = ui.allocate_exact_size(vec2(largura, 48.0), Sense::click());
                let resposta = resposta.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Clique para pôr no editor");
                let pintor = ui.painter();
                if resposta.hovered() {
                    pintor.rect_filled(rect.shrink2(vec2(4.0, 2.0)), CornerRadius::same(tema::RAIO_ETIQUETA), p.realce);
                }
                let hora = c.hora.as_str();
                let y1 = rect.top() + 14.0;
                let x = rect.left() + 22.0;
                // Cancelada por você não é erro: anel neutro e "Cancelada · 1,8 s".
                let cancelada = c.resultado == "cancelada";
                if c.erro && !cancelada {
                    tema::ponto(pintor, pos2(rect.left() + 9.0, y1), 3.5, tema::EstadoVisual::Erro);
                } else if matches!(c.resultado.as_str(), "recusada" | "expirou" | "cancelada") {
                    pintor.circle_stroke(pos2(rect.left() + 9.0, y1), 2.75, Stroke::new(1.5, p.suave));
                }
                pintor.text(pos2(x, y1), Align2::LEFT_CENTER, hora, FontId::monospace(11.5), p.suave);
                let primeira = c.sql.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim().to_string();
                let job = {
                    let mut j = sql::realce(dialeto, &primeira, FontId::monospace(12.5), p, None);
                    j.wrap = egui::text::TextWrapping {
                        max_width: (rect.right() - x - 52.0).max(20.0),
                        max_rows: 1,
                        break_anywhere: true,
                        overflow_character: Some('…'),
                    };
                    j
                };
                let g = pintor.layout_job(job);
                pintor.galley(pos2(x + 52.0, y1 - g.size().y / 2.0), g, p.texto);
                let mut partes = Vec::new();
                if cancelada && c.origem != "agente" {
                    partes.push(format!("Cancelada · {}", duracao_curta(c.duracao_ms)));
                } else if c.erro {
                    partes.push("Erro".to_string());
                }
                if c.altera {
                    partes.push(if c.linhas == 1 { "1 linha alterada".into() } else { format!("{} linhas alteradas", c.linhas) });
                } else if !c.erro && (c.resultado.is_empty() || c.resultado == "aprovada") {
                    partes.push(if c.linhas == 1 { "1 linha".into() } else { format!("{} linhas", c.linhas) });
                }
                if c.resultado.is_empty() || c.resultado == "aprovada" {
                    partes.push(format!("{} ms", c.duracao_ms));
                }
                if c.origem == "agente" {
                    let quem = agentes(c.agente_id);
                    partes.push(match c.resultado.as_str() {
                        "recusada" => format!("{quem} · você recusou"),
                        "expirou" => format!("{quem} · expirou sem resposta"),
                        "cancelada" => format!("{quem} · cancelada"),
                        _ => format!("{quem} · aprovado por você"),
                    });
                } else {
                    partes.push("você".into());
                }
                if !c.banco.is_empty() {
                    partes.push(format!("banco {}", c.banco));
                }
                pintor.text(pos2(x, rect.top() + 34.0), Align2::LEFT_CENTER, partes.join(" · "), FontId::proportional(11.5), p.suave);
                if resposta.clicked() {
                    inserir = Some(c.sql.clone());
                }
            }
        });
        if let Some(t) = inserir {
            let ctx = ui.ctx().clone();
            self.inserir(&ctx, &t);
        }
    }
}

/// Onde inserir (o fim da linha do cursor) e o texto: uma linha em branco
/// antes (se o editor não está vazio) e o `;` no fim.
fn texto_inserido(atual: &str, byte_cursor: usize, texto: &str) -> (usize, String) {
    let fim_da_linha = atual[byte_cursor..].find('\n').map_or(atual.len(), |n| byte_cursor + n);
    let mut inserido = String::new();
    if !atual.trim().is_empty() {
        inserido.push_str(if atual[..fim_da_linha].ends_with('\n') { "\n" } else { "\n\n" });
    } else {
        // Editor só com espaços: o texto entra no começo.
        return (0, format!("{};", texto.trim().trim_end_matches(';').trim_end()));
    }
    inserido.push_str(texto.trim().trim_end_matches(';').trim_end());
    inserido.push(';');
    (fim_da_linha, inserido)
}

/// "850 ms", "1,8 s", "2 min 5 s".
fn duracao_curta(ms: i64) -> String {
    match ms {
        ..1000 => format!("{ms} ms"),
        1000..60_000 => format!("{:.1} s", ms as f64 / 1000.0).replace('.', ","),
        _ => format!("{} min {} s", ms / 60_000, (ms % 60_000) / 1000),
    }
}

/// A sigla do tipo na barra do console.
pub fn sigla_do_tipo(tipo: &str) -> &'static str {
    match tipo {
        "postgres" => "PG",
        "mysql" => "MySQL",
        "sqlserver" => "MSSQL",
        _ => "SQLite",
    }
}

fn nome_do_tipo(tipo: &str) -> &'static str {
    match tipo {
        "postgres" => "PostgreSQL",
        "mysql" => "MySQL/MariaDB",
        "sqlserver" => "SQL Server",
        _ => "SQLite",
    }
}

pub fn id_editor() -> Id {
    Id::new("editor-sql")
}

/// As colunas que aparecem entre `esquerda` e `direita`: (índice, x, largura).
fn colunas_visiveis(larguras: &[f32], x0: f32, esquerda: f32, direita: f32) -> Vec<(usize, f32, f32)> {
    let mut x = x0;
    let mut lista = Vec::new();
    for (c, w) in larguras.iter().enumerate() {
        if x + w >= esquerda && x <= direita {
            lista.push((c, x, *w));
        }
        x += w;
    }
    lista
}

/// O texto de uma célula numa linha só: quebra de linha e tabulação viram
/// ↵ e →, e outro caractere de controle vira · (a fonte não tem desenho para
/// eles). Devolve também se havia algum desses outros, para pintar apagado.
fn texto_da_celula(v: &str) -> (String, bool) {
    let mut controle = false;
    let t = v
        .chars()
        .filter(|c| *c != '\r')
        .map(|c| match c {
            '\n' => '↵',
            '\t' => '→',
            c if c.is_control() => {
                controle = true;
                '·'
            }
            c => c,
        })
        .collect();
    (t, controle)
}

/// Casas decimais de cada coluna de números (a maior da página), para os
/// pontos ficarem na mesma vertical.
fn casas_decimais(r: &api::Resultado) -> Vec<usize> {
    (0..r.colunas.len())
        .map(|c| {
            if !r.colunas[c].numero {
                return 0;
            }
            r.linhas
                .iter()
                .filter_map(|l| l.get(c).and_then(|v| v.as_deref()))
                .filter(|v| !v.contains(['e', 'E']))
                .map(|v| v.split_once('.').map_or(0, |(_, d)| d.len()))
                .max()
                .unwrap_or(0)
                .min(12)
        })
        .collect()
}

/// Completa com espaços à direita até `casas` (em fonte mono, os pontos
/// alinham). O valor não muda.
fn alinhar_decimal(v: &str, casas: usize) -> String {
    if casas == 0 || v.contains(['e', 'E']) {
        return v.to_string();
    }
    let falta = match v.split_once('.') {
        Some((_, d)) => casas.saturating_sub(d.len()),
        None => casas + 1,
    };
    format!("{v}{}", " ".repeat(falta))
}

/// As larguras das colunas, medidas na primeira página (até 400 px).
fn medir_larguras(r: &api::Resultado) -> Vec<f32> {
    // Mono 12,5 tem ~7,5 px por caractere; o nome em negrito, ~7,5 também.
    let por_caractere = 7.6;
    r.colunas
        .iter()
        .enumerate()
        .map(|(c, col)| {
            let mut maior =
                col.nome.chars().count() as f32 * por_caractere + if col.tipo.is_empty() { 0.0 } else { 6.0 + col.tipo.chars().count() as f32 * 6.2 };
            for l in r.linhas.iter().take(200) {
                let n = l.get(c).and_then(|v| v.as_ref()).map_or(4, |v| v.chars().count().min(60));
                maior = maior.max(n as f32 * por_caractere);
            }
            (maior + 16.0).clamp(48.0, 400.0)
        })
        .collect()
}

/// As duas dicas do painel sem nada executado, centradas.
fn estado_vazio(ui: &mut egui::Ui, corpo: Rect) {
    let p = cores();
    let pintor = ui.painter();
    let fonte = FontId::proportional(13.0);
    let g1 = pintor.layout_no_wrap("executa a instrução sob o cursor ou a seleção.".into(), fonte.clone(), p.suave);
    let tecla = pintor.layout_no_wrap("Ctrl+Enter".into(), FontId::monospace(12.5), p.texto);
    let largura_tecla = tecla.size().x + 16.0;
    let largura = largura_tecla + 6.0 + g1.size().x;
    let topo = corpo.center().y - 24.0;
    let x = corpo.center().x - largura / 2.0;
    let r_tecla = Rect::from_min_size(pos2(x, topo), vec2(largura_tecla, 22.0));
    pintor.rect(r_tecla, CornerRadius::same(tema::RAIO_ETIQUETA), p.superficie, Stroke::new(1.0, p.borda), StrokeKind::Inside);
    pintor.galley(r_tecla.center() - tecla.size() / 2.0, tecla, p.texto);
    pintor.galley(pos2(r_tecla.right() + 6.0, r_tecla.center().y - g1.size().y / 2.0), g1, p.suave);
    pintor.text(pos2(corpo.center().x, topo + 22.0 + 6.0 + 9.0), Align2::CENTER_CENTER, "Clique duplo numa tabela mostra as primeiras linhas.", fonte, p.suave);
}

/// Bloco de erro, aviso ou informação no topo do corpo. Devolve se a ação
/// foi clicada. Com `mono`, o texto todo é a mensagem do servidor (mono 12,5).
#[allow(clippy::too_many_arguments)]
/// Devolve o índice do botão de `acoes` clicado (o primeiro é o principal).
fn bloco(ui: &mut egui::Ui, corpo: Rect, texto: &str, acoes: &[&str], cor: Color32, anel: bool, etiqueta: Option<&str>, mono: bool) -> Option<usize> {
    let p = cores();
    let area = corpo.shrink(12.0);
    let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(area));
    let mut clicou = None;
    egui::ScrollArea::vertical().id_salt("bloco-resultado").max_height(area.height()).show(&mut filho, |ui| {
        egui::Frame::new()
            .fill(tema::fundo_tingido(p, cor, tema::claro()))
            .corner_radius(CornerRadius::same(tema::RAIO_CONTROLE))
            .inner_margin(egui::Margin::symmetric(12, 10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_top(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(8.0, 18.0), Sense::hover());
                    if anel {
                        ui.painter().circle_stroke(r.center(), 2.75, Stroke::new(1.5, cor));
                    } else {
                        ui.painter().circle_filled(r.center(), 3.5, cor);
                    }
                    ui.vertical(|ui| {
                        let mut linhas = texto.lines();
                        if let Some(primeira) = linhas.next() {
                            let primeira = RichText::new(primeira).color(p.texto);
                            ui.label(if mono { primeira.font(FontId::monospace(12.5)) } else { primeira.size(13.0) });
                        }
                        let resto: Vec<&str> = linhas.collect();
                        if !resto.is_empty() {
                            ui.add_space(4.0);
                            ui.label(RichText::new(resto.join("\n")).color(p.texto).font(FontId::monospace(12.5)));
                        }
                        if let Some(e) = etiqueta {
                            // Neutra: vermelho sobre o fundo tingido de vermelho não se lê.
                            ui.add_space(4.0);
                            let g = ui.painter().layout_no_wrap(e.to_owned(), tema::fonte_etiqueta(), p.texto);
                            let (r, _) = ui.allocate_exact_size(vec2(g.size().x + 12.0, 19.0), Sense::hover());
                            ui.painter().rect(r, CornerRadius::same(tema::RAIO_ETIQUETA), p.superficie_alta, Stroke::new(1.0, p.borda), StrokeKind::Inside);
                            ui.painter().galley(r.center() - g.size() / 2.0, g, p.texto);
                        }
                        if !acoes.is_empty() {
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                for (i, a) in acoes.iter().enumerate() {
                                    if tema::botao_secundario(ui, a).clicked() {
                                        clicou = Some(i);
                                    }
                                }
                            });
                        }
                    });
                });
            });
    });
    clicou
}

/// Ícone de parar (para o menu e o histórico, se precisar).
#[allow(dead_code)]
pub fn icone_parar(pintor: &egui::Painter, c: egui::Pos2) {
    tema::desenhar_icone(pintor, c, Icone::Parar, cores().erro);
}

#[cfg(test)]
mod testes {
    use super::*;

    fn resultado() -> api::Resultado {
        api::Resultado {
            colunas: vec![
                api::ColunaResultado { nome: "id".into(), tipo: "int4".into(), numero: true },
                api::ColunaResultado { nome: "nome".into(), ..Default::default() },
            ],
            linhas: vec![vec![Some("1".into()), Some("cliente-x".into())], vec![Some("2".into()), None]],
            ..Default::default()
        }
    }

    #[test]
    fn copia_em_tsv_com_null() {
        let mut c = Console::novo("");
        c.resultado_novo(resultado(), "f".into(), String::new(), None);
        assert_eq!(c.texto_da_selecao(Selecao::Linha(1)).unwrap(), "2\tNULL");
        assert_eq!(c.texto_da_selecao(Selecao::Celula(0, 1)).unwrap(), "cliente-x");
        assert_eq!(c.texto_da_selecao(Selecao::Tudo).unwrap(), "id\tnome\n1\tcliente-x\n2\tNULL");
        assert_eq!(c.texto_da_coluna(1).unwrap(), "nome\ncliente-x\nNULL");
    }

    #[test]
    fn larguras_entre_48_e_400() {
        let mut r = resultado();
        r.linhas.push(vec![Some("1".into()), Some("x".repeat(500))]);
        let l = medir_larguras(&r);
        assert!(l[0] >= 48.0 && l[1] <= 400.0);
    }

    #[test]
    fn historico_entra_separado_e_com_ponto_e_virgula() {
        let atual = "select pg_sleep(20)";
        let (onde, inserido) = texto_inserido(atual, atual.len(), "select id from clientes where id > 3");
        let mut t = atual.to_string();
        t.insert_str(onde, &inserido);
        assert_eq!(t, "select pg_sleep(20)\n\nselect id from clientes where id > 3;");
        // O Ctrl+Enter com o cursor antes do ";" pega só a inserida.
        let cursor = onde + inserido.len() - 1;
        assert_eq!(&t[sql::sob_cursor("postgres", &t, cursor).unwrap()], "select id from clientes where id > 3");
        assert_eq!(texto_inserido("", 0, "select 1;"), (0, "select 1;".to_string()));
    }

    #[test]
    fn celulas_com_controle_e_decimais() {
        assert_eq!(texto_da_celula("a\tb\r\nc"), ("a→b↵c".to_string(), false));
        assert_eq!(texto_da_celula("\u{1}\u{2}"), ("··".to_string(), true));
        let mut r = resultado();
        r.linhas = vec![vec![Some("914.1".into()), None], vec![Some("506.51".into()), None], vec![Some("7".into()), None]];
        let casas = casas_decimais(&r);
        assert_eq!(casas, vec![2, 0]);
        assert_eq!(alinhar_decimal("914.1", 2), "914.1 ");
        assert_eq!(alinhar_decimal("7", 2), "7   ");
        assert_eq!(alinhar_decimal("1e10", 2), "1e10");
    }

    #[test]
    fn tempos_em_texto() {
        assert_eq!(duracao_curta(1757), "1,8 s");
        assert_eq!(duracao_curta(850), "850 ms");
        assert_eq!(texto_tempo(30), "30 s");
        assert_eq!(texto_tempo(300), "5 min");
    }
}
