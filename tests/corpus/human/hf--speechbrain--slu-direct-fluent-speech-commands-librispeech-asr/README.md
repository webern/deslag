---
language: en
thumbnail:
tags:
- speechbrain
- Spoken language understanding
license: cc0-1.0
datasets:
- Fluent Speech Commands
metrics:
- Accuracy
---

<iframe src="https://ghbtns.com/github-btn.html?user=speechbrain&repo=speechbrain&type=star&count=true&size=large&v=2" frameborder="0" scrolling="0" width="170" height="30" title="GitHub"></iframe>
<br/><br/>

# Fluent Speech Commands
The dataset contains real recordings that define a simple spoken language understanding task. You can download it from [here](https://fluent.ai/fluent-speech-commands-a-dataset-for-spoken-language-understanding-research/). 
The Fluent Speech Commands dataset contains 30,043 utterances from 97 speakers. It is recorded as 16 kHz single-channel .wav files each containing a single utterance used for controlling smart-home appliances or virtual assistant, for example, “put on the music” or “turn up the heat in the kitchen”. Each audio is labeled with three slots: action, object, and location. A slot takes on one of the multiple values: for instance, the “location” slot can take on the values “none”, “kitchen”, “bedroom”, or “washroom”. We refer to the combination of slot values as the intent of the utterance. For each intent, there are multiple possible wordings: for example, the intent {action: “activate”, object: “lights”, location: “none”} can be expressed as “turn on the lights”, “switch the lights on”, “lights on”, etc. The dataset has a total of 248 phrasing mapping to 31 unique intents.

# End-to-end SLU model for Fluent Speech Commands
Attention-based RNN sequence-to-sequence model for the [Fluent Speech Commands](https://arxiv.org/pdf/1904.03670.pdf) dataset. 
This model checkpoint achieves 99.6% accuracy on the test set.
The model uses an ASR model trained on LibriSpeech ([`speechbrain/asr-crdnn-rnnlm-librispeech`](https://huggingface.co/speechbrain/asr-crdnn-rnnlm-librispeech)) to extract features from the input audio, then maps these features to an intent and slot labels using a beam search. 


You can try the model on the `example_fsc.wav` file included here as follows:

```python
from speechbrain.pretrained import EndToEndSLU
slu = EndToEndSLU.from_hparams("/network/tmp1/ravanelm/slu-direct-fluent-speech-commands-librispeech-asr")
# Text: "Please, turn on the light of the bedroom"
slu.decode_file("/network/tmp1/ravanelm/slu-direct-fluent-speech-commands-librispeech-asr/example_fsc.wav")
>>> '{"action:" "activate"| "object": "lights"| "location": "bedroom"}'
```
The system is trained with recordings sampled at 16kHz (single channel).
The code will automatically normalize your audio (i.e., resampling + mono channel selection) when calling *decode_file* if needed. Make sure your input tensor is compliant with the expected sampling rate if you use *encode_batch* and *decode_batch*.
### Inference on GPU
To perform inference on the GPU, add  `run_opts={"device":"cuda"}`  when calling the `from_hparams` method.

### Training
The model was trained with SpeechBrain (f1f421b3).
To train it from scratch follows these steps:
1. Clone SpeechBrain:
```bash
git clone https://github.com/speechbrain/speechbrain/
```
2. Install it:
```
cd speechbrain
pip install -r requirements.txt
pip install -e .
```

3. Run Training:
```
cd recipes/fluent-speech-commands
python train.py hparams/train.yaml --data_folder=your_data_folder
```

You can find our training results (models, logs, etc) [here](https://drive.google.com/drive/folders/1Zly54252Z218IHJQ9M0B3kTQPZIw_2yC?usp=sharing).

### Limitations
The SpeechBrain team does not provide any warranty on the performance achieved by this model when used on other datasets.

#### Referencing SpeechBrain

```bibtex
@misc{speechbrain,
  title={{SpeechBrain}: A General-Purpose Speech Toolkit},
  author={Mirco Ravanelli and Titouan Parcollet and Peter Plantinga and Aku Rouhe and Samuele Cornell and Loren Lugosch and Cem Subakan and Nauman Dawalatabad and Abdelwahab Heba and Jianyuan Zhong and Ju-Chieh Chou and Sung-Lin Yeh and Szu-Wei Fu and Chien-Feng Liao and Elena Rastorgueva and François Grondin and William Aris and Hwidong Na and Yan Gao and Renato De Mori and Yoshua Bengio},
  year={2021},
  eprint={2106.04624},
  archivePrefix={arXiv},
  primaryClass={eess.AS},
  note={arXiv:2106.04624}
}
```

#### Referencing Fluent Speech Commands

```bibtex
@inproceedings{fluent,
  author    = {Loren Lugosch and
               Mirco Ravanelli and
               Patrick Ignoto and
               Vikrant Singh Tomar and
               Yoshua Bengio},
  editor    = {Gernot Kubin and
               Zdravko Kacic},
  title     = {Speech Model Pre-Training for End-to-End Spoken Language Understanding},
  booktitle = {Proc. of Interspeech},
  pages     = {814--818},
  year      = {2019},
}
```

#### About SpeechBrain
SpeechBrain is an open-source and all-in-one speech toolkit. It is designed to be simple, extremely flexible, and user-friendly. Competitive or state-of-the-art performance is obtained in various domains.

Website: https://speechbrain.github.io/

GitHub: https://github.com/speechbrain/speechbrain