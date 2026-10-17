//! A conversa com a tela (ou a abelha fixa): uma linha de JSON por mensagem,
//! pela entrada e pela saída padrão deste processo. Nada passa por rede.

use std::path::PathBuf;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// O que a tela pede. Uma linha maior que isto é recusada.
pub const LIMITE_LINHA: usize = 4096;

#[derive(Debug, PartialEq, Eq)]
pub enum Comando {
    /// Abrir o microfone e começar a guardar o áudio (na memória). Com
    /// `ate_silencio`, ele fecha sozinho na pausa depois da fala (um comando).
    Ouvir { ate_silencio: bool },
    /// Fechar o microfone e transcrever o que ouviu.
    Parar,
    /// Fechar o microfone e jogar fora o que ouviu.
    Cancelar,
    /// Falar um texto com a voz da abelha (com o efeito leve, se pedido).
    Falar { texto: String, efeito: bool },
    /// Parar de falar na hora.
    Calar,
    /// Ficar ouvindo o nome (as referências do arquivo, ou as do nome
    /// padrão) e, ouvindo, abrir o microfone para o comando.
    Vigiar { referencias: Option<PathBuf>, sensibilidade: u8 },
    /// Parar de ouvir o nome (o microfone fecha).
    Desvigiar,
    /// Gravar uma vez o nome (na memória), até a pausa.
    Amostra,
    /// Aprender o nome com as gravações e guardar as referências (não o áudio).
    Aprender { nome: String, destino: PathBuf },
    /// Jogar fora as gravações do nome.
    EsquecerAmostras,
    /// Sair agora.
    Sair,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(tag = "evento", rename_all = "snake_case")]
pub enum Evento {
    /// O processo está pronto para receber pedidos.
    Iniciado,
    /// O microfone abriu.
    Ouvindo,
    /// O microfone fechou e a transcrição começou.
    Transcrevendo {
        segundos: f32,
    },
    /// O texto transcrito, já limpo (pode vir vazio).
    Texto {
        texto: String,
    },
    /// Algo deu errado; a mensagem é para quem usa.
    Erro {
        mensagem: String,
    },
    /// O microfone ficou aberto ouvindo o nome (ou fechou).
    Vigiando {
        ligado: bool,
    },
    /// Ouviu o nome: o microfone segue aberto para o comando.
    Chamado,
    /// A voz da abelha começou e terminou.
    Falando,
    Falou,
    /// Uma gravação do nome ficou guardada (na memória).
    Amostra {
        numero: usize,
        segundos: f32,
    },
    /// As referências do nome foram guardadas.
    Aprendido,
    /// O processo vai sair (`parado`: muito tempo sem uso).
    Saindo {
        motivo: &'static str,
    },
}

#[derive(Deserialize)]
struct SoComando {
    comando: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SemCampos {
    #[allow(dead_code)]
    comando: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OuvirJson {
    #[allow(dead_code)]
    comando: String,
    #[serde(default)]
    ate_silencio: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FalarJson {
    #[allow(dead_code)]
    comando: String,
    texto: String,
    #[serde(default)]
    efeito: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VigiarJson {
    #[allow(dead_code)]
    comando: String,
    #[serde(default)]
    referencias: Option<PathBuf>,
    #[serde(default = "sensibilidade_padrao")]
    sensibilidade: u8,
}

fn sensibilidade_padrao() -> u8 {
    5
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AprenderJson {
    #[allow(dead_code)]
    comando: String,
    nome: String,
    destino: PathBuf,
}

fn campos<T: DeserializeOwned>(linha: &str) -> Result<T, String> {
    serde_json::from_str(linha).map_err(|_| "Pedido desconhecido para a voz.".to_string())
}

/// Lê um pedido: `{"comando": "...", ...}` com só os campos daquele comando.
pub fn ler(linha: &str) -> Result<Comando, String> {
    if linha.len() > LIMITE_LINHA {
        return Err("Pedido grande demais para a voz.".into());
    }
    let linha = linha.trim();
    let nome = serde_json::from_str::<SoComando>(linha).map_err(|_| "Pedido desconhecido para a voz.".to_string())?.comando;
    let simples = |c: Comando| campos::<SemCampos>(linha).map(|_| c);
    match nome.as_str() {
        "ouvir" => campos::<OuvirJson>(linha).map(|o| Comando::Ouvir { ate_silencio: o.ate_silencio }),
        "parar" => simples(Comando::Parar),
        "cancelar" => simples(Comando::Cancelar),
        "falar" => campos::<FalarJson>(linha).map(|f| Comando::Falar { texto: f.texto, efeito: f.efeito }),
        "calar" => simples(Comando::Calar),
        "vigiar" => campos::<VigiarJson>(linha).map(|v| Comando::Vigiar { referencias: v.referencias, sensibilidade: v.sensibilidade.clamp(1, 9) }),
        "desvigiar" => simples(Comando::Desvigiar),
        "amostra" => simples(Comando::Amostra),
        "aprender" => campos::<AprenderJson>(linha).map(|a| Comando::Aprender { nome: a.nome, destino: a.destino }),
        "esquecer_amostras" => simples(Comando::EsquecerAmostras),
        "sair" => simples(Comando::Sair),
        _ => Err("Pedido desconhecido para a voz.".into()),
    }
}

pub fn linha(evento: &Evento) -> String {
    // Um Evento sempre vira JSON: só tem textos e números.
    serde_json::to_string(evento).unwrap_or_default()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn le_os_comandos_conhecidos() {
        assert_eq!(ler(r#"{"comando":"ouvir"}"#), Ok(Comando::Ouvir { ate_silencio: false }));
        assert_eq!(ler(r#"{"comando":"ouvir","ate_silencio":true}"#), Ok(Comando::Ouvir { ate_silencio: true }));
        assert_eq!(ler(" {\"comando\":\"parar\"}\n"), Ok(Comando::Parar));
        assert_eq!(ler(r#"{"comando":"cancelar"}"#), Ok(Comando::Cancelar));
        assert_eq!(ler(r#"{"comando":"sair"}"#), Ok(Comando::Sair));
        assert_eq!(ler(r#"{"comando":"falar","texto":"Oi."}"#), Ok(Comando::Falar { texto: "Oi.".into(), efeito: false }));
        assert_eq!(ler(r#"{"comando":"falar","texto":"Oi.","efeito":true}"#), Ok(Comando::Falar { texto: "Oi.".into(), efeito: true }));
        assert_eq!(ler(r#"{"comando":"calar"}"#), Ok(Comando::Calar));
        assert_eq!(ler(r#"{"comando":"vigiar"}"#), Ok(Comando::Vigiar { referencias: None, sensibilidade: 5 }));
        assert_eq!(
            ler(r#"{"comando":"vigiar","referencias":"/d/voz/nome-1.json","sensibilidade":30}"#),
            Ok(Comando::Vigiar { referencias: Some("/d/voz/nome-1.json".into()), sensibilidade: 9 })
        );
        assert_eq!(ler(r#"{"comando":"desvigiar"}"#), Ok(Comando::Desvigiar));
        assert_eq!(ler(r#"{"comando":"amostra"}"#), Ok(Comando::Amostra));
        assert_eq!(
            ler(r#"{"comando":"aprender","nome":"Zuzu","destino":"/d/voz/nome-1.json"}"#),
            Ok(Comando::Aprender { nome: "Zuzu".into(), destino: "/d/voz/nome-1.json".into() })
        );
        assert_eq!(ler(r#"{"comando":"esquecer_amostras"}"#), Ok(Comando::EsquecerAmostras));
    }

    #[test]
    fn recusa_o_resto() {
        for l in [
            "",
            "ouvir",
            r#"{"comando":"gravar"}"#,
            r#"{"comando":"ouvir","arquivo":"/tmp/x"}"#,
            r#"{"comando":"parar","texto":"x"}"#,
            r#"{"comando":"falar"}"#,
            r#"{"comando":"falar","texto":3}"#,
            r#"{"comando":"aprender","nome":"x"}"#,
            "[1]",
        ] {
            assert!(ler(l).is_err(), "{l}");
        }
        let grande = format!(r#"{{"comando":"falar","texto":"{}"}}"#, "a".repeat(LIMITE_LINHA));
        assert!(ler(&grande).is_err());
    }

    #[test]
    fn escreve_uma_linha_de_json() {
        assert_eq!(linha(&Evento::Texto { texto: "olá \"mundo\"".into() }), r#"{"evento":"texto","texto":"olá \"mundo\""}"#);
        assert_eq!(linha(&Evento::Saindo { motivo: "parado" }), r#"{"evento":"saindo","motivo":"parado"}"#);
        assert_eq!(linha(&Evento::Iniciado), r#"{"evento":"iniciado"}"#);
        assert_eq!(linha(&Evento::Vigiando { ligado: true }), r#"{"evento":"vigiando","ligado":true}"#);
        assert_eq!(linha(&Evento::Amostra { numero: 2, segundos: 0.5 }), r#"{"evento":"amostra","numero":2,"segundos":0.5}"#);
    }
}
