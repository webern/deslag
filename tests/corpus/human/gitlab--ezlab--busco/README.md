## BUSCOv5 - Benchmarking sets of Universal Single-Copy Orthologs.

For full documentation please consult the user guide: https://busco.ezlab.org/busco_userguide.html

Main changes in v5:

- Metaeuk is used as default gene predictor for eukaryote pipeline. Augustus is maintained and can be used optionally instead of Metaeuk.
- Introduction of batch mode: input argument can be a folder containing input files
- The folder structure has changed, so if doing a manual installation, make sure to completely remove any previous versions of BUSCO before installing v5.

***
### Installation

#### Conda
Conda installation instructions are in the userguide here:
https://busco.ezlab.org/busco_userguide.html#conda-package

#### Docker
BUSCO is available through DockerHub - instructions here:
https://busco.ezlab.org/busco_userguide.html#docker-image

#### Manual installation
Manual installation is possible, though it is important to validate each of the dependencies before running BUSCO. 
More details in the user guide: https://busco.ezlab.org/busco_userguide.html#manual-installation

***
### Troubleshooting
To get help with BUSCO use: ``busco -h`` and ``python3 scripts/generate_plot.py -h``

Report problems on the BUSCO issue board at https://gitlab.com/ezlab/busco/issues

***
### How to cite BUSCO

If you have used BUSCO v4 or V5 in your analyses, please cite:

Manni M, Berkeley MR, Seppey M, Simão FA, Zdobnov EM. 2021. BUSCO update: novel and streamlined workflows along with broader and deeper phylogenetic coverage for scoring of eukaryotic, prokaryotic, and viral genomes. Molecular Biology and Evolution [Internet]. Available from: https://doi.org/10.1093/molbev/msab199

Previous publications:
BUSCO: Assessing Genome Assembly and Annotation Completeness. Mathieu Seppey, Mosè Manni, Evgeny M. Zdobnov In: Kollmar M. (eds) Gene Prediction. Methods in Molecular Biology, vol 1962. Humana, New York, NY. 2019 doi.org/10.1007/978-1-4939-9173-0_14

BUSCO applications from quality assessments to gene prediction and phylogenomics. Robert M. Waterhouse, Mathieu Seppey, Felipe A. Simão, Mosè Manni, Panagiotis Ioannidis, Guennadi Klioutchnikov, Evgenia V. Kriventseva, and Evgeny M. Zdobnov Mol Biol Evol, published online Dec 6, 2017 doi: 10.1093/molbev/msx319

BUSCO: assessing genome assembly and annotation completeness with single-copy orthologs. Felipe A. Simão, Robert M. Waterhouse, Panagiotis Ioannidis, Evgenia V. Kriventseva, and Evgeny M. Zdobnov Bioinformatics, published online June 9, 2015 doi: 10.1093/bioinformatics/btv351

Copyright (c) 2016-2021, Evgeny Zdobnov (ez@ezlab.org)
Licensed under the MIT license. See LICENSE.md file.
