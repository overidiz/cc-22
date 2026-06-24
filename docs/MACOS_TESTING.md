# CC-22 — teste no macOS

Este pacote universal contém versões nativas para Apple Silicon e Intel:

- `CC-22.vst3` — copie para `~/Library/Audio/Plug-Ins/VST3/`
- `CC-22.clap` — copie para `~/Library/Audio/Plug-Ins/CLAP/`
- `CC-22.app` — mova para `/Applications` ou execute da pasta desejada

O CC-22 ainda não é notarizado pela Apple. Se o macOS bloquear a primeira
abertura, abra o Terminal na pasta extraída e execute:

```sh
xattr -cr CC-22.app CC-22.vst3 CC-22.clap
```

Depois reinicie o DAW e force uma nova varredura dos plugins. O formato AU não
está incluído; para este teste use um host VST3 ou CLAP.

## Checklist do testador

1. Informe modelo do Mac, versão do macOS, chip e DAW.
2. Abra o standalone, autorize a entrada de áudio e confirme entrada/saída.
3. Carregue o VST3 ou CLAP em uma faixa estéreo.
4. Abra e feche a interface cinco vezes; redimensione a janela do host.
5. Troque todos os modos e presets enquanto o áudio toca.
6. Arraste os quatro módulos para reordenar a cadeia.
7. Teste bypass global, bypass individual, dry/wet e automação.
8. Salve o projeto, feche o DAW, reabra e confirme a restauração do estado.
9. Salve um snapshot e confirme o arquivo em
   `~/Library/Application Support/CC-22/Presets/`.
10. Relate qualquer crash, pico de CPU, silêncio, estalo ou interface cortada.
