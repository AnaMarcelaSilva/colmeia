//! Caixa de mensagem do painel da tarefa. Enter quebra a linha, Ctrl+Enter
//! envia, e Ctrl+V com uma imagem na área de transferência anexa a imagem: ela
//! vai para o núcleo como PNG (a tela não escreve nos dados), fica anexada à
//! tarefa, e o caminho que o núcleo devolve vai junto na mensagem, que é como
//! o Claude Code e o Codex recebem imagens.
//!
//! A seta para cima traz as mensagens já enviadas ao agente em foco, como no
//! terminal. O histórico fica no núcleo (sobrevive a fechar a Colmeia) e é
//! lido uma vez por agente, numa thread; nada é consultado por tempo.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, Sender};

use eframe::egui::{self, ColorImage, CornerRadius, Event, Key, Modifiers, TextureHandle, TextureOptions, vec2};

use crate::api;
use crate::dados::AgenteTela;
use crate::tema::{self, cores};

/// Mensagens guardadas por agente (o núcleo guarda o mesmo tanto).
const MAX_HISTORICO: usize = 200;

/// Navegação pelo histórico de um agente, sem nada de tela (dá para testar).
/// `itens` vai da mais nova para a mais antiga.
#[derive(Default, Debug)]
pub struct Historico {
    itens: Vec<String>,
    posicao: Option<usize>,
    rascunho_guardado: String,
}

impl Historico {
    pub fn novo(itens: Vec<String>) -> Self {
        Historico { itens, ..Default::default() }
    }

    pub fn len(&self) -> usize {
        self.itens.len()
    }

    pub fn navegando(&self) -> bool {
        self.posicao.is_some()
    }

    /// "Mensagem 3 de 41": a posição (contando da mais nova) e o total.
    pub fn onde(&self) -> Option<(usize, usize)> {
        self.posicao.map(|p| (p + 1, self.itens.len()))
    }

    /// Seta para cima: a primeira guarda o rascunho e traz a mais nova; as
    /// outras vão para as mais antigas. Na mais antiga, fica nela (None).
    pub fn subir(&mut self, atual: &str) -> Option<String> {
        let proxima = match self.posicao {
            None if self.itens.is_empty() => return None,
            None => {
                self.rascunho_guardado = atual.to_string();
                0
            }
            Some(p) if p + 1 < self.itens.len() => p + 1,
            Some(_) => return None,
        };
        self.posicao = Some(proxima);
        Some(self.itens[proxima].clone())
    }

    /// Seta para baixo: volta para as mais novas e, depois da mais nova,
    /// devolve o rascunho e encerra a navegação.
    pub fn descer(&mut self) -> Option<String> {
        match self.posicao? {
            0 => self.cancelar(),
            p => {
                self.posicao = Some(p - 1);
                Some(self.itens[p - 1].clone())
            }
        }
    }

    /// Esc: devolve o rascunho e encerra a navegação.
    pub fn cancelar(&mut self) -> Option<String> {
        self.posicao.take()?;
        Some(std::mem::take(&mut self.rascunho_guardado))
    }

    /// O texto foi editado: ele passa a ser o rascunho, sem navegação.
    pub fn editou(&mut self) {
        self.posicao = None;
        self.rascunho_guardado.clear();
    }

    /// Uma mensagem enviada entra na frente (a repetição seguida não entra).
    pub fn acrescentar(&mut self, texto: &str) {
        self.editou();
        if self.itens.first().is_some_and(|t| t == texto) {
            return;
        }
        self.itens.insert(0, texto.to_string());
        self.itens.truncate(MAX_HISTORICO);
    }

    /// Tira a mais nova se for este texto (o núcleo não guardou).
    pub fn desfazer(&mut self, texto: &str) {
        if self.itens.first().is_some_and(|t| t == texto) {
            self.itens.remove(0);
        }
    }
}

/// O que as threads do histórico respondem.
enum RespostaHistorico {
    Lido(i64, Result<Vec<String>, String>),
    /// O núcleo não guardou (parece ter senha ou chave).
    NaoGuardada(i64, String),
}

/// Maior imagem aceita (em pixels), para uma colagem acidental não travar a tela.
const MAIOR_IMAGEM: usize = 40_000_000;
const LADO_MINIATURA: f32 = 52.0;

struct Anexo {
    id: i64,
    caminho: String,
    miniatura: TextureHandle,
}

pub struct Compositor {
    rascunho: String,
    pub para_todos: bool,
    pub focar: bool,
    anexos: Vec<Anexo>,
    aviso: Option<String>,
    /// Quando houve a última colagem de texto, para não tratar o mesmo Ctrl+V como imagem.
    colou_texto_em: f64,
    /// Histórico de cada agente, lido do núcleo na primeira vez que ele fica em foco.
    historicos: HashMap<i64, Historico>,
    lendo: HashSet<i64>,
    respostas: (Sender<RespostaHistorico>, Receiver<RespostaHistorico>),
    /// "Não ficou no histórico…": some na próxima edição, não por tempo.
    aviso_historico: Option<String>,
    /// A caixa tinha o teclado no quadro anterior (o egui solta o foco no
    /// Esc antes de a caixa ver a tecla).
    tinha_foco: bool,
}

impl Default for Compositor {
    fn default() -> Self {
        Compositor {
            rascunho: String::new(),
            para_todos: false,
            focar: false,
            anexos: Vec::new(),
            aviso: None,
            colou_texto_em: 0.0,
            historicos: HashMap::new(),
            lendo: HashSet::new(),
            respostas: mpsc::channel(),
            aviso_historico: None,
            tinha_foco: false,
        }
    }
}

/// Altura da linha de dica embaixo da caixa.
pub const ALTURA_DICA: f32 = 24.0;
const ID: &str = "compositor";

/// O que enviar e para quem.
pub struct Envio {
    pub destinos: Vec<i64>,
    pub texto: String,
}

impl Compositor {
    /// Altura que a caixa precisa: cresce com o texto até 5 linhas, mais a
    /// fileira de imagens e a linha de dica.
    pub fn altura(&self) -> f32 {
        let linhas = self.rascunho.split('\n').count().clamp(1, 5) as f32;
        let imagens = if self.anexos.is_empty() { 0.0 } else { LADO_MINIATURA + 10.0 };
        54.0 + 19.0 * (linhas - 1.0) + imagens + ALTURA_DICA
    }

    /// O teclado está na caixa de mensagem.
    /// Põe uma citação (como "@src/main.rs") no fim da mensagem em escrita e
    /// leva o teclado para a caixa.
    pub fn citar(&mut self, texto: &str) {
        if !self.rascunho.is_empty() && !self.rascunho.ends_with([' ', '\n']) {
            self.rascunho.push(' ');
        }
        self.rascunho.push_str(texto);
        self.rascunho.push(' ');
        self.focar = true;
    }

    pub fn com_foco(ctx: &egui::Context) -> bool {
        ctx.memory(|m| m.has_focus(egui::Id::new(ID)))
    }

    /// Quantas mensagens do agente estão guardadas (0 se ainda não foi lido).
    pub fn tamanho_historico(&self, agente: i64) -> usize {
        self.historicos.get(&agente).map_or(0, Historico::len)
    }

    /// O agente saiu (ou o histórico dele foi apagado): esquece o que estava aqui.
    pub fn esquecer(&mut self, agente: i64) {
        self.historicos.remove(&agente);
    }

    /// Histórico apagado no núcleo: fica vazio aqui também, sem ler de novo.
    pub fn historico_limpo(&mut self, agente: i64) {
        self.historicos.insert(agente, Historico::default());
    }

    /// Lê o histórico do agente em foco uma vez, numa thread.
    fn ler_historico(&mut self, ctx: &egui::Context, agente: i64) {
        if agente == 0 || self.historicos.contains_key(&agente) || !self.lendo.insert(agente) {
            return;
        }
        let envio = self.respostas.0.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let lido = api::mensagens(agente).map(|l| l.into_iter().map(|m| m.texto).collect());
            let _ = envio.send(RespostaHistorico::Lido(agente, lido));
            ctx.request_repaint();
        });
    }

    fn receber_historicos(&mut self) {
        while let Ok(r) = self.respostas.1.try_recv() {
            match r {
                RespostaHistorico::Lido(agente, resultado) => {
                    self.lendo.remove(&agente);
                    match resultado {
                        Ok(itens) => {
                            self.historicos.entry(agente).or_insert_with(|| Historico::novo(itens));
                        }
                        // O histórico é conveniência: um erro fica só no log.
                        Err(e) => eprintln!("histórico de mensagens do agente {agente}: {e}"),
                    }
                }
                RespostaHistorico::NaoGuardada(agente, texto) => {
                    if let Some(h) = self.historicos.get_mut(&agente) {
                        h.desfazer(&texto);
                    }
                    self.aviso_historico = Some("Enviada. Não ficou no histórico porque parece ter uma senha ou chave.".into());
                }
            }
        }
    }

    /// Guarda o texto enviado no histórico de cada destino: aqui na hora, no núcleo numa thread.
    fn guardar_no_historico(&mut self, ctx: &egui::Context, destinos: &[i64], texto: &str) {
        if texto.trim().is_empty() {
            return;
        }
        for &agente in destinos {
            if let Some(h) = self.historicos.get_mut(&agente) {
                h.acrescentar(texto);
            }
            let envio = self.respostas.0.clone();
            let (ctx, texto) = (ctx.clone(), texto.to_string());
            std::thread::spawn(move || match api::guardar_mensagem(agente, &texto) {
                Ok(false) => {
                    let _ = envio.send(RespostaHistorico::NaoGuardada(agente, texto));
                    ctx.request_repaint();
                }
                Ok(true) => {}
                Err(e) => eprintln!("guardando a mensagem do agente {agente}: {e}"),
            });
        }
    }

    /// Setas e Esc do histórico, tiradas da fila antes de o campo ver. A seta
    /// para cima só entra na navegação com o cursor na primeira linha; já
    /// navegando, as duas setas andam em qualquer linha (um item antigo de
    /// várias linhas não prende a navegação). Com Shift, Ctrl ou Alt, nunca.
    fn teclas_do_historico(&mut self, ui: &egui::Ui, id: egui::Id, agente: i64) {
        let Some(historico) = self.historicos.get_mut(&agente) else { return };
        let ctx = ui.ctx();
        let sem_modificador = ui.input(|i| i.modifiers.is_none());
        if !sem_modificador {
            return;
        }
        let cursor = egui::TextEdit::load_state(ctx, id).and_then(|s| s.cursor.char_range()).map_or(self.rascunho.chars().count(), |r| r.primary.index.into());
        let antes: String = self.rascunho.chars().take(cursor).collect();
        let primeira_linha = !antes.contains('\n');
        let navegando = historico.navegando();
        let mut novo = None;
        let tecla = |k| ctx.input_mut(|i| i.consume_key(Modifiers::NONE, k));
        if (navegando || (primeira_linha && historico.len() > 0)) && tecla(Key::ArrowUp) {
            novo = historico.subir(&self.rascunho);
        } else if navegando && tecla(Key::ArrowDown) {
            novo = historico.descer();
        } else if navegando && tecla(Key::Escape) {
            novo = historico.cancelar();
            // O Esc já tirou o foco da caixa; ele volta para continuar o rascunho.
            ctx.memory_mut(|m| m.request_focus(id));
        }
        if let Some(texto) = novo {
            self.rascunho = texto;
            // Cursor no fim do texto trazido.
            let mut estado = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
            let fim = egui::text::CCursor::new(self.rascunho.chars().count());
            estado.cursor.set_char_range(Some(egui::text::CCursorRange::one(fim)));
            estado.store(ctx, id);
        }
    }

    pub fn mostrar(&mut self, ui: &mut egui::Ui, area: egui::Rect, agentes: &[AgenteTela], foco: i64, tarefa: i64) -> Option<Envio> {
        let p = cores();
        let id = egui::Id::new(ID);
        // O clique que pediu o foco (um botão da lousa, por exemplo) solta o
        // botão neste quadro, e o egui tiraria o foco de volta: pede no próximo.
        if self.focar && ui.input(|i| i.pointer.any_released()) {
            ui.ctx().request_repaint();
        } else if std::mem::take(&mut self.focar) {
            ui.memory_mut(|m| m.request_focus(id));
        }
        let com_foco = ui.memory(|m| m.has_focus(id));
        self.receber_historicos();
        self.ler_historico(ui.ctx(), foco);
        let navegando_antes = self.historicos.get(&foco).is_some_and(Historico::navegando);
        if com_foco || (self.tinha_foco && navegando_antes) {
            self.teclas_do_historico(ui, id, foco);
        }
        self.tinha_foco = ui.memory(|m| m.has_focus(id));
        let navegando = self.historicos.get(&foco).and_then(Historico::onde);
        // Ctrl+Enter envia; é retirado da fila antes de o campo ver, para não virar quebra de linha.
        let mut enviar = com_foco && ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Enter));
        // O egui só avisa um Ctrl+V quando há texto para colar. Sem texto (só uma
        // imagem), o que chega é a tecla V sendo solta com o Ctrl ainda apertado.
        let (agora, colou_texto, soltou_ctrl_v) = ui.input(|i| {
            (
                i.time,
                i.events.iter().any(|e| matches!(e, Event::Paste(_))),
                i.events.iter().any(|e| matches!(e, Event::Key { key: Key::V, pressed: false, modifiers, .. } if modifiers.command)),
            )
        });
        if colou_texto {
            self.colou_texto_em = agora;
        }
        if com_foco && soltou_ctrl_v && agora - self.colou_texto_em > 0.5 {
            self.colar_imagem(ui.ctx(), tarefa);
        }

        let caixa = egui::Rect::from_min_size(area.min, vec2(area.width(), area.height() - ALTURA_DICA));
        // Navegando no histórico, a borda é a do foco, mais leve: o texto é um item antigo.
        let contorno = match (navegando, com_foco) {
            (Some(_), _) => egui::Stroke::new(1.5, p.destaque.gamma_multiply(0.55)),
            (None, true) => egui::Stroke::new(1.5, p.destaque),
            (None, false) => egui::Stroke::new(1.0, p.borda),
        };
        ui.painter().rect(caixa, CornerRadius::same(18), p.superficie_alta, contorno, egui::StrokeKind::Inside);

        let mut interno = caixa.shrink2(vec2(10.0, 10.0));
        if !self.anexos.is_empty() {
            let fileira = egui::Rect::from_min_size(interno.min + vec2(4.0, 0.0), vec2(interno.width(), LADO_MINIATURA));
            self.miniaturas(ui, fileira);
            interno.min.y += LADO_MINIATURA + 10.0;
        }

        let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(interno).layout(egui::Layout::left_to_right(egui::Align::Min)));
        let em_foco = agentes.iter().find(|a| a.id == foco).map_or_else(String::new, |a| format!("{} · {}", a.nome(), a.papel));
        let destino = if self.para_todos { format!("todos ({})", agentes.len()) } else { em_foco.clone() };
        let resposta = tema::chip(&mut filho, "Para", &destino, self.para_todos);
        egui::Popup::menu(&resposta).show(|ui| {
            ui.set_min_width(250.0);
            if tema::opcao_menu(ui, &format!("{em_foco} (em foco)"), !self.para_todos) {
                self.para_todos = false;
                ui.close();
            }
            if tema::opcao_menu(ui, &format!("Todos os {} agentes da tarefa", agentes.len()), self.para_todos) {
                self.para_todos = true;
                ui.close();
            }
        });
        filho.add_space(6.0);
        // Só um agente rodando recebe mensagem: o texto de um parado se perderia.
        let destinos: Vec<i64> = agentes.iter().filter(|a| a.ativo && (self.para_todos || a.id == foco)).map(|a| a.id).collect();
        let tem_conteudo = !self.rascunho.trim().is_empty() || !self.anexos.is_empty();
        let pode_enviar = tem_conteudo && !destinos.is_empty();
        filho.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            if tema::botao_principal(ui, "Enviar", pode_enviar).clicked() {
                enviar = true;
            }
            ui.add_space(8.0);
            let dica = if self.para_todos { "Mensagem para todos os agentes da tarefa".to_string() } else { format!("Mensagem para {em_foco}") };
            let campo = egui::TextEdit::multiline(&mut self.rascunho)
                .id(id)
                // A margem vai na moldura: com uma moldura dada, o TextEdit
                // ignora `.margin()`, e o texto ficava 7 px acima de "Para:".
                .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(4, 7)))
                .desired_rows(1)
                .desired_width(ui.available_width())
                .font(egui::FontId::proportional(14.0))
                .hint_text(dica);
            if ui.add(campo).changed() {
                // Qualquer edição encerra a navegação: o texto vira o rascunho.
                if let Some(h) = self.historicos.get_mut(&foco) {
                    h.editou();
                }
                self.aviso_historico = None;
            }
        });

        let parou = if self.para_todos {
            "Nenhum agente da tarefa está rodando. Inicie um para mandar mensagens."
        } else {
            "O agente parou. Inicie de novo para mandar mensagens."
        };
        let tem_historico = self.tamanho_historico(foco) > 0;
        let (texto_dica, cor_dica) = match (&self.aviso, navegando, &self.aviso_historico) {
            (Some(aviso), _, _) => (aviso.clone(), p.erro),
            (None, _, _) if destinos.is_empty() => (parou.to_string(), p.alerta),
            (None, Some((n, total)), _) => (format!("Mensagem {n} de {total} · ↓ mais nova · Esc volta ao rascunho"), p.suave),
            (None, None, Some(aviso)) => (aviso.clone(), p.alerta),
            (None, None, None) if tem_historico => {
                ("Ctrl+Enter envia · ↑ mensagens anteriores · Enter quebra a linha · Ctrl+V cola imagens".to_string(), p.suave)
            }
            (None, None, None) => ("Ctrl+Enter envia · Enter quebra a linha · Ctrl+V cola imagens".to_string(), p.suave),
        };
        ui.painter().text(
            egui::pos2(caixa.left() + 14.0, caixa.bottom() + 12.0),
            egui::Align2::LEFT_CENTER,
            texto_dica,
            egui::FontId::proportional(11.5),
            cor_dica,
        );

        if !(enviar && pode_enviar) {
            return None;
        }
        let mut texto = self.rascunho.trim().to_string();
        // Só o que foi digitado entra no histórico (o caminho de uma imagem antiga não serve de novo).
        self.guardar_no_historico(ui.ctx(), &destinos, &texto);
        self.aviso_historico = None;
        if !self.anexos.is_empty() {
            let caminhos: Vec<String> = self.anexos.iter().map(|a| a.caminho.clone()).collect();
            let rotulo = if caminhos.len() == 1 { "Imagem anexada:" } else { "Imagens anexadas:" };
            texto = format!("{texto}\n\n{rotulo}\n{}", caminhos.join("\n")).trim().to_string();
        }
        self.rascunho.clear();
        self.anexos.clear();
        self.aviso = None;
        ui.memory_mut(|m| m.request_focus(id));
        Some(Envio { destinos, texto })
    }

    fn miniaturas(&mut self, ui: &mut egui::Ui, fileira: egui::Rect) {
        let p = cores();
        let mut remover = None;
        for (i, anexo) in self.anexos.iter().enumerate() {
            let caixa = egui::Rect::from_min_size(fileira.min + vec2(i as f32 * (LADO_MINIATURA + 8.0), 0.0), vec2(LADO_MINIATURA, LADO_MINIATURA));
            let tamanho = anexo.miniatura.size_vec2();
            // Preenche o quadrado cortando o excesso, sem distorcer a imagem.
            let escala = (LADO_MINIATURA / tamanho.x).max(LADO_MINIATURA / tamanho.y);
            let visivel = vec2(LADO_MINIATURA / (tamanho.x * escala), LADO_MINIATURA / (tamanho.y * escala));
            let uv = egui::Rect::from_center_size(egui::pos2(0.5, 0.5), visivel);
            egui::Image::new(&anexo.miniatura).uv(uv).corner_radius(8).paint_at(ui, caixa);
            ui.painter().rect_stroke(caixa, 8, egui::Stroke::new(1.0, p.borda), egui::StrokeKind::Inside);
            let x = egui::Rect::from_center_size(caixa.right_top() + vec2(-6.0, 6.0), vec2(18.0, 18.0));
            let resposta = ui.interact(x, ui.id().with(("remover-anexo", i)), egui::Sense::click()).on_hover_text("Tirar imagem");
            ui.painter().circle_filled(x.center(), 9.0, if resposta.hovered() { p.erro } else { p.texto.gamma_multiply(0.75) });
            ui.painter().text(x.center(), egui::Align2::CENTER_CENTER, "×", egui::FontId::proportional(13.0), p.superficie_alta);
            if resposta.clicked() {
                remover = Some(i);
            }
        }
        if let Some(i) = remover {
            let anexo = self.anexos.remove(i);
            let _ = crate::api::remover_anexo(anexo.id);
        }
    }

    fn colar_imagem(&mut self, ctx: &egui::Context, tarefa: i64) {
        let Ok(mut area) = arboard::Clipboard::new() else {
            return;
        };
        // Sem imagem na área de transferência é uma colagem de texto comum.
        let Ok(imagem) = area.get_image() else { return };
        if imagem.width * imagem.height > MAIOR_IMAGEM {
            self.aviso = Some("Imagem grande demais para anexar.".into());
            return;
        }
        let enviado = codificar_png(&imagem).and_then(|png| crate::api::anexar(tarefa, "mensagem", None, &png));
        match enviado {
            Ok(anexo) => {
                let cor = ColorImage::from_rgba_unmultiplied([imagem.width, imagem.height], &imagem.bytes);
                let miniatura = ctx.load_texture(format!("anexo-{}", anexo.id), cor, TextureOptions::LINEAR);
                self.anexos.push(Anexo { id: anexo.id, caminho: anexo.caminho, miniatura });
                self.aviso = None;
            }
            Err(e) => self.aviso = Some(format!("Não consegui anexar a imagem: {e}")),
        }
    }
}

/// Codifica a imagem colada como PNG, para enviar ao núcleo.
fn codificar_png(imagem: &arboard::ImageData) -> Result<Vec<u8>, String> {
    let mut png = Vec::new();
    let mut codificador = png::Encoder::new(&mut png, imagem.width as u32, imagem.height as u32);
    codificador.set_color(png::ColorType::Rgba);
    codificador.set_depth(png::BitDepth::Eight);
    let mut escritor = codificador.write_header().map_err(|e| e.to_string())?;
    escritor.write_image_data(&imagem.bytes).map_err(|e| e.to_string())?;
    escritor.finish().map_err(|e| e.to_string())?;
    Ok(png)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn historico() -> Historico {
        Historico::novo(vec!["terceira".into(), "segunda\ncom duas linhas".into(), "primeira".into()])
    }

    #[test]
    fn sobe_e_desce_guardando_o_rascunho() {
        let mut h = historico();
        assert_eq!(h.subir("meu rascunho").as_deref(), Some("terceira"));
        assert_eq!(h.onde(), Some((1, 3)));
        assert_eq!(h.subir("terceira").as_deref(), Some("segunda\ncom duas linhas"));
        assert_eq!(h.subir("x").as_deref(), Some("primeira"));
        // Na mais antiga, fica nela.
        assert_eq!(h.subir("primeira"), None);
        assert_eq!(h.onde(), Some((3, 3)));
        assert_eq!(h.descer().as_deref(), Some("segunda\ncom duas linhas"));
        assert_eq!(h.descer().as_deref(), Some("terceira"));
        // Depois da mais nova, volta o rascunho e a navegação acaba.
        assert_eq!(h.descer().as_deref(), Some("meu rascunho"));
        assert!(!h.navegando());
        assert_eq!(h.descer(), None);
    }

    #[test]
    fn esc_devolve_o_rascunho() {
        let mut h = historico();
        h.subir("rascunho");
        h.subir("terceira");
        assert_eq!(h.cancelar().as_deref(), Some("rascunho"));
        assert!(!h.navegando());
        assert_eq!(h.cancelar(), None);
    }

    #[test]
    fn editar_encerra_a_navegacao() {
        let mut h = historico();
        h.subir("rascunho");
        h.editou();
        assert!(!h.navegando());
        // A próxima seta para cima guarda o texto editado como rascunho.
        assert_eq!(h.subir("terceira editada").as_deref(), Some("terceira"));
        assert_eq!(h.cancelar().as_deref(), Some("terceira editada"));
    }

    #[test]
    fn lista_vazia_nao_navega() {
        let mut h = Historico::default();
        assert_eq!(h.subir("x"), None);
        assert!(!h.navegando());
        assert_eq!(h.descer(), None);
    }

    #[test]
    fn enviada_entra_na_frente_sem_repetir() {
        let mut h = historico();
        h.subir("x");
        h.acrescentar("nova");
        assert!(!h.navegando());
        h.acrescentar("nova");
        assert_eq!(h.len(), 4);
        assert_eq!(h.subir("").as_deref(), Some("nova"));
        h.desfazer("outra");
        assert_eq!(h.len(), 4);
        h.desfazer("nova");
        assert_eq!(h.len(), 3);
        let mut cheio = Historico::novo((0..MAX_HISTORICO).map(|i| i.to_string()).collect());
        cheio.acrescentar("mais uma");
        assert_eq!(cheio.len(), MAX_HISTORICO);
    }
}
