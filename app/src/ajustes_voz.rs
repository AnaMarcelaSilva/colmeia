//! Ajustes › Comandos falados e voz da abelha: cada item ligado à parte e
//! tudo desligado por padrão. Os comandos usam o Whisper (o mesmo do ditado),
//! a voz da abelha baixa o Kokoro, e o "ouvir o nome" deixa o microfone da
//! abelha fixa aberto, com a sensibilidade e, para um nome que não é o
//! padrão, 3 ou 4 gravações feitas aqui (que viram referências, não áudio).
//! A pergunta livre com IA é uma opção à parte no menu (o mesmo diálogo, só
//! com ela): é a única que usa tokens.

use std::sync::Arc;

use canal::voz::{Conversa, NomeOuvido, Saida};
use eframe::egui::{self, RichText};
use serde_json::json;

use crate::api;
use crate::dialogos::{Resultado, megabytes};
use crate::tema::{self, cores};

/// Uma sessão de gravação do nome, com um colmeia-voz só para ela.
struct Gravacao {
    conversa: Conversa,
    feitas: usize,
    ouvindo: bool,
}

pub struct AjustesVoz {
    perfil: i64,
    pub opcoes: api::OpcoesVoz,
    abelha_fixa: bool,
    nome_abelha: String,
    whisper: api::EstadoVoz,
    fala: api::EstadoVoz,
    nome: Option<NomeOuvido>,
    gravacao: Option<Gravacao>,
    aviso: Option<String>,
    erro: Option<String>,
    /// Só a pergunta livre com IA (a opção à parte no menu).
    so_pergunta: bool,
}

impl AjustesVoz {
    /// A opção à parte: só a pergunta livre, sem perguntar dos modelos ao núcleo.
    pub fn pergunta(perfil: &api::Perfil) -> Self {
        AjustesVoz {
            perfil: perfil.id,
            opcoes: perfil.voz.clone(),
            abelha_fixa: perfil.abelha_fixa,
            nome_abelha: perfil.mascote_nome.clone(),
            whisper: api::EstadoVoz::default(),
            fala: api::EstadoVoz::default(),
            nome: None,
            gravacao: None,
            aviso: None,
            erro: None,
            so_pergunta: true,
        }
    }

    pub fn new(perfil: &api::Perfil) -> Self {
        let mut erro = None;
        let mut ler = |r: Result<api::EstadoVoz, String>| {
            r.unwrap_or_else(|e| {
                erro = Some(format!("Não consegui falar com o núcleo: {e}"));
                api::EstadoVoz::default()
            })
        };
        let whisper = ler(api::estado_voz());
        let fala = ler(api::estado_fala());
        AjustesVoz {
            perfil: perfil.id,
            opcoes: perfil.voz.clone(),
            abelha_fixa: perfil.abelha_fixa,
            nome_abelha: perfil.mascote_nome.clone(),
            whisper,
            fala,
            nome: canal::voz::nome_ouvido(perfil.id).ok(),
            gravacao: None,
            aviso: None,
            erro,
            so_pergunta: false,
        }
    }

    /// O progresso de um download (pelo evento do núcleo).
    pub fn progresso(&mut self, pacote: &str, e: api::EstadoVoz) {
        match pacote {
            "kokoro" => self.fala = e,
            _ => self.whisper = e,
        }
    }

    fn mudar(&mut self, campos: serde_json::Value) {
        self.erro = api::definir_voz(self.perfil, campos).err().map(|e| format!("Não consegui mudar: {e}"));
    }
}

fn secao(ui: &mut egui::Ui, titulo: &str, texto: &str) {
    ui.add_space(16.0);
    ui.label(tema::texto_forte(titulo, 14.0).color(cores().texto));
    ui.add_space(3.0);
    ui.label(RichText::new(texto).size(12.5).color(cores().suave));
    ui.add_space(8.0);
}

fn barra(ui: &mut egui::Ui, e: &api::EstadoVoz) {
    let fracao = e.baixados as f32 / e.tamanho.max(1) as f32;
    ui.add(egui::ProgressBar::new(fracao).fill(cores().destaque).desired_height(8.0));
    ui.label(RichText::new(format!("{} de {}", megabytes(e.baixados), megabytes(e.tamanho))).color(cores().suave).size(12.0));
}

/// Mostra o diálogo; as mudanças vão ao núcleo e voltam pelo evento perfil.voz.
pub fn mostrar(ui: &mut egui::Ui, d: &mut AjustesVoz) -> Resultado {
    let mut resultado = Resultado::Continua;
    if d.so_pergunta {
        return mostrar_pergunta(ui, d);
    }
    receber_gravacao(ui.ctx(), d);
    tema::cabecalho(
        ui,
        "Comandos falados e voz da abelha",
        "Fale com a abelha para rodar e parar projetos, abrir e encerrar tarefas e saber o que espera por você. Os comandos rodam neste computador, sem tokens, e o áudio não é gravado.",
    );
    egui::ScrollArea::vertical().max_height(520.0).show(ui, |ui| {
        comandos(ui, d);
        voz_da_abelha(ui, d);
        if ouvir_o_nome(ui, d) {
            resultado = Resultado::OuvirNome;
        }
    });
    if let Some(a) = &d.aviso {
        ui.add_space(8.0);
        ui.label(RichText::new(a).color(cores().suave).size(12.5));
    }
    if let Some(e) = &d.erro {
        ui.add_space(8.0);
        ui.label(RichText::new(e).color(cores().erro).size(12.5));
    }
    ui.add_space(16.0);
    if tema::botao_secundario(ui, "Fechar").clicked() {
        return Resultado::Fechar;
    }
    resultado
}

fn comandos(ui: &mut egui::Ui, d: &mut AjustesVoz) {
    secao(
        ui,
        "Comandos falados",
        &format!(
            "Clique na abelha (ou {}) e fale: \u{201c}rodar a loja web\u{201d}, \u{201c}parar…\u{201d}, \u{201c}abrir a tarefa…\u{201d}, \u{201c}encerrar a tarefa…\u{201d}, \u{201c}como está a tarefa…?\u{201d} ou \u{201c}o que espera por mim?\u{201d}. O microfone fecha sozinho na pausa. Parar e encerrar pedem confirmação. O resumo da abelha fica no botão direito.",
            crate::falada::ATALHO
        ),
    );
    let (pronto, baixando) = (d.whisper.estado == "pronto", d.whisper.estado == "baixando");
    if baixando {
        barra(ui, &d.whisper);
    }
    let ligado = d.opcoes.voz_comandos;
    let rotulo = match (ligado, pronto, baixando) {
        (true, _, _) => "Desligar os comandos".to_string(),
        (_, _, true) => "Cancelar download".to_string(),
        (_, true, _) => "Ligar os comandos".to_string(),
        _ => format!("Baixar {} e ligar", megabytes(d.whisper.tamanho.max(1))),
    };
    if !pronto && !baixando && !ligado {
        ui.label(
            RichText::new("Os comandos usam o Whisper base (o mesmo do ditado), baixado uma vez do repositório oficial e conferido antes de usar.")
                .size(12.0)
                .color(cores().suave),
        );
        ui.add_space(6.0);
    }
    let clicou = if ligado || baixando { tema::botao_alerta(ui, &rotulo).clicked() } else { tema::botao_principal(ui, &rotulo, true).clicked() };
    if !clicou {
        return;
    }
    d.erro = None;
    match (ligado, pronto, baixando) {
        (true, _, _) => d.mudar(json!({ "voz_comandos": false })),
        (_, _, true) => d.erro = api::cancelar_voz().err(),
        (_, true, _) => d.mudar(json!({ "voz_comandos": true })),
        _ => match api::baixar_voz_e_ligar(d.perfil, "comandos") {
            Ok(e) => d.whisper = e,
            Err(e) => d.erro = Some(format!("Não consegui começar o download: {e}")),
        },
    }
}

fn voz_da_abelha(ui: &mut egui::Ui, d: &mut AjustesVoz) {
    secao(ui, "Voz da abelha", "A resposta também é falada, com uma voz feminina em português (Kokoro, voz pf_dora), feita neste computador.");
    if d.fala.estado == "indisponivel" {
        ui.label(RichText::new("Ainda não disponível neste sistema: a resposta aparece só no balão.").size(12.5).color(cores().suave));
        return;
    }
    let (pronto, baixando) = (d.fala.estado == "pronto", d.fala.estado == "baixando");
    if baixando {
        barra(ui, &d.fala);
    }
    if d.fala.estado == "erro" && !d.fala.erro.is_empty() {
        ui.label(RichText::new(&d.fala.erro).color(cores().erro).size(12.5));
    }
    let ligada = d.opcoes.voz_abelha;
    let mut efeito = d.opcoes.voz_efeito;
    let (mut acao, mut apagar) = (false, false);
    ui.horizontal(|ui| {
        let rotulo = match (ligada, pronto, baixando) {
            (true, _, _) => "Desligar a voz".to_string(),
            (_, _, true) => "Cancelar download".to_string(),
            (_, true, _) => "Ligar a voz".to_string(),
            _ => format!("Baixar {} e ligar", megabytes(d.fala.tamanho.max(1))),
        };
        acao = if ligada || baixando { tema::botao_alerta(ui, &rotulo).clicked() } else { tema::botao_principal(ui, &rotulo, d.opcoes.voz_comandos).clicked() };
        if pronto && !baixando {
            apagar = tema::botao_secundario(ui, &format!("Apagar a voz ({})", megabytes(d.fala.tamanho)))
                .on_hover_text("Libera o espaço; a voz desliga em todos os perfis.")
                .clicked();
        }
    });
    if !pronto && !baixando && !ligada {
        ui.label(
            RichText::new("Baixa uma vez o modelo Kokoro-82M (8 bits), a voz e o ONNX Runtime, das fontes oficiais, conferidos antes de usar.")
                .size(12.0)
                .color(cores().suave),
        );
    }
    if ligada {
        ui.add_space(6.0);
        if tema::caixa_marcar(ui, "Efeito leve na voz (brilho e eco curto)", &mut efeito).changed() {
            d.mudar(json!({ "voz_efeito": efeito }));
        }
    }
    if apagar {
        match api::remover_fala() {
            Ok(e) => d.fala = e,
            Err(e) => d.erro = Some(format!("Não consegui apagar a voz: {e}")),
        }
    } else if acao {
        match (ligada, pronto, baixando) {
            (true, _, _) => d.mudar(json!({ "voz_abelha": false })),
            (_, _, true) => d.erro = api::cancelar_fala().err(),
            (_, true, _) => d.mudar(json!({ "voz_abelha": true })),
            _ => match api::baixar_fala(d.perfil) {
                Ok(e) => d.fala = e,
                Err(e) => d.erro = Some(format!("Não consegui começar o download: {e}")),
            },
        }
    }
}

/// Devolve `true` quando pediu para ligar (a janela abre a abelha fixa e liga).
fn ouvir_o_nome(ui: &mut egui::Ui, d: &mut AjustesVoz) -> bool {
    let nome = d.nome_abelha.clone();
    secao(
        ui,
        "Ouvir o nome (opcional)",
        &format!(
            "A abelha fixa fica ouvindo \u{201c}{nome}\u{201d} para abrir o microfone sem clique: diga o nome e o comando. O microfone fica aberto o tempo todo, com um ponto vermelho na abelha, e o som só passa pela memória."
        ),
    );
    let Some(n) = d.nome.clone() else {
        ui.label(RichText::new("Não consegui saber do núcleo como está o nome.").size(12.5).color(cores().suave));
        return false;
    };
    let pode_ouvir = n.padrao || n.gravado;
    if n.padrao {
        ui.label(RichText::new(format!("\u{201c}{nome}\u{201d} já vem com reconhecimento pronto.")).size(12.5).color(cores().texto));
    } else {
        gravar_o_nome(ui, d, &n);
    }
    ui.add_space(8.0);
    let ligado = d.opcoes.voz_nome;
    let rotulo = if ligado { "Parar de ouvir o nome" } else { "Ouvir o nome" };
    let pode = d.opcoes.voz_comandos && pode_ouvir;
    let clicou = if ligado { tema::botao_alerta(ui, rotulo).clicked() } else { tema::botao_principal(ui, rotulo, pode).clicked() };
    if !d.opcoes.voz_comandos {
        ui.label(RichText::new("Ligue os comandos falados primeiro.").size(12.0).color(cores().suave));
    } else if !d.abelha_fixa && !ligado {
        ui.label(RichText::new("Ligar fixa a abelha na área de trabalho: é ela que ouve.").size(12.0).color(cores().suave));
    }
    let mut ligar = false;
    if clicou {
        if ligado {
            d.mudar(json!({ "voz_nome": false }));
        } else {
            ligar = true;
            d.abelha_fixa = true;
        }
    }
    ui.add_space(10.0);
    let s = d.opcoes.voz_sensibilidade.clamp(1, 9);
    ui.label(RichText::new("Sensibilidade: mais alta ouve mais fácil (e se engana mais)").size(12.5).color(cores().texto));
    ui.add_space(4.0);
    const NIVEIS: [&str; 9] = ["1", "2", "3", "4", "5", "6", "7", "8", "9"];
    if let Some(i) = tema::segmentado(ui, &NIVEIS, s as usize - 1) {
        let nova = i as u8 + 1;
        if nova != s {
            d.opcoes.voz_sensibilidade = nova;
            d.mudar(json!({ "voz_sensibilidade": nova }));
        }
    }
    ligar
}

/// A pergunta livre com IA: separada das outras e avisando que gasta tokens.
fn mostrar_pergunta(ui: &mut egui::Ui, d: &mut AjustesVoz) -> Resultado {
    tema::cabecalho(
        ui,
        "Pergunta livre com IA",
        "Pergunte à abelha sobre o andamento do trabalho, com as suas palavras. Desligada por padrão porque usa tokens.",
    );
    secao(
        ui,
        "Como funciona",
        "O que não for um dos comandos vira uma pergunta sobre o andamento, como \u{201c}o que eu fiz hoje?\u{201d} ou \u{201c}o que falta na sprint?\u{201d}, respondida pelo Claude Code com a conta deste perfil. Cada pergunta gasta tokens. Ele recebe só um retrato curto das tarefas, dos agentes, das frases da reunião e do que aconteceu hoje, nunca o terminal, senhas ou bancos, e nada é guardado. Dá para falar (com os comandos ligados) ou escrever no resumo da abelha.",
    );
    let ligada = d.opcoes.voz_ia;
    let clicou = if ligada {
        tema::botao_alerta(ui, "Desligar a pergunta livre").clicked()
    } else {
        tema::botao_principal(ui, "Ligar a pergunta livre", true).clicked()
    };
    if clicou {
        d.erro = None;
        d.mudar(json!({ "voz_ia": !ligada }));
    }
    if let Some(e) = &d.erro {
        ui.add_space(8.0);
        ui.label(RichText::new(e).color(cores().erro).size(12.5));
    }
    ui.add_space(16.0);
    if tema::botao_secundario(ui, "Fechar").clicked() {
        return Resultado::Fechar;
    }
    Resultado::Continua
}

/// Um nome que não é o padrão: 3 ou 4 gravações, uma de cada vez.
fn gravar_o_nome(ui: &mut egui::Ui, d: &mut AjustesVoz, n: &NomeOuvido) {
    let nome = &n.nome;
    if n.gravado && d.gravacao.is_none() {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("As gravações de \u{201c}{nome}\u{201d} estão prontas.")).size(12.5).color(cores().texto));
            if tema::botao_secundario(ui, "Gravar de novo").clicked() {
                comecar_gravacao(ui.ctx(), d);
            }
            if tema::botao_secundario(ui, "Apagar as gravações").clicked() {
                match canal::voz::esquecer_nome(d.perfil) {
                    Ok(novo) => d.nome = Some(novo),
                    Err(e) => d.erro = Some(format!("Não consegui apagar: {e}")),
                }
            }
        });
        return;
    }
    let feitas = d.gravacao.as_ref().map_or(0, |g| g.feitas);
    let ouvindo = d.gravacao.as_ref().is_some_and(|g| g.ouvindo);
    ui.label(
        RichText::new(format!(
            "Para reconhecer \u{201c}{nome}\u{201d}, grave o nome {} ou {} vezes, uma de cada vez, do jeito que vai chamar. Gravações feitas: {feitas}.",
            3, 4
        ))
        .size(12.5)
        .color(cores().texto),
    );
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let rotulo = if ouvindo { "Ouvindo… diga o nome".to_string() } else { format!("Gravar ({} de 4)", (feitas + 1).min(4)) };
        if tema::botao_principal(ui, &rotulo, !ouvindo).clicked() {
            if d.gravacao.is_none() {
                comecar_gravacao(ui.ctx(), d);
            }
            if let Some(g) = d.gravacao.as_mut() {
                match g.conversa.amostra() {
                    Ok(()) => g.ouvindo = true,
                    Err(e) => d.erro = Some(e),
                }
            }
        }
        if feitas >= 3 && !ouvindo && tema::botao_principal(ui, "Aprender o nome", true).clicked() {
            let destino = n.arquivo.clone();
            if let Some(g) = d.gravacao.as_mut()
                && let Err(e) = g.conversa.aprender(nome, &destino)
            {
                d.erro = Some(e);
            }
        }
        if ouvindo {
            // O microfone aberto fica sempre à vista.
            ui.label(RichText::new("● Microfone ligado").size(12.0).color(cores().erro));
        }
    });
}

fn comecar_gravacao(ctx: &egui::Context, d: &mut AjustesVoz) {
    let ctx = ctx.clone();
    d.gravacao = Some(Gravacao { conversa: Conversa::nova(d.perfil, Arc::new(move || ctx.request_repaint())), feitas: 0, ouvindo: false });
    d.aviso = None;
}

fn receber_gravacao(_ctx: &egui::Context, d: &mut AjustesVoz) {
    let Some(g) = d.gravacao.as_mut() else { return };
    let mut aprendeu = false;
    for s in g.conversa.receber() {
        match s {
            Saida::Amostra(n) => {
                g.ouvindo = false;
                g.feitas = n;
            }
            Saida::Erro(e) => {
                g.ouvindo = false;
                d.aviso = Some(e);
            }
            Saida::Aprendido => aprendeu = true,
            _ => {}
        }
    }
    if aprendeu {
        // As gravações saem da memória junto com o programa.
        d.gravacao = None;
        d.nome = canal::voz::nome_ouvido(d.perfil).ok();
        d.aviso = Some("Pronto: o nome foi aprendido. Agora dá para ligar o ouvir o nome.".into());
    }
}
