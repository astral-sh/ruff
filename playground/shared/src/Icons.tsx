// Icons from the [VS code icon set](https://github.com/microsoft/vscode-icons/).
//
// Icon-LICENSE:
// MIT License
//
// Copyright (c) Microsoft Corporation.
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE

export function File({
  width = 24,
  height = 24,
}: {
  width?: number;
  height?: number;
}) {
  return (
    <svg
      width={width}
      height={height}
      viewBox="0 0 16 16"
      xmlns="http://www.w3.org/2000/svg"
    >
      <path
        fillRule="evenodd"
        clipRule="evenodd"
        d="M10.57 1.14L13.85 4.44L14 4.8V14.5L13.5 15H2.5L2 14.5V1.5L2.5 1H10.22L10.57 1.14ZM10 5H13L10 2V5ZM3 2V14H13V6H9.5L9 5.5V2H3ZM5.062 9.533L6.879 7.705L6.17 7L4 9.179V9.886L6.171 12.06L6.878 11.353L5.062 9.533ZM8.8 7.714L9.5 7.005L11.689 9.18V9.889L9.5 12.062L8.795 11.353L10.626 9.533L8.8 7.714Z"
      />
    </svg>
  );
}

export function Run({
  width = 24,
  height = 24,
}: {
  width?: number;
  height?: number;
}) {
  return (
    <svg
      width={width}
      height={height}
      viewBox="0 0 16 16"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
    >
      <path
        fillRule="evenodd"
        clipRule="evenodd"
        d="M4.00024 2V14.4805L12.9149 8.24024L4.00024 2ZM11.1812 8.24024L4.99524 12.5684V3.91209L11.1812 8.24024Z"
        fill="#ffffff"
      />
    </svg>
  );
}

export function Settings() {
  return (
    <svg
      width="24"
      height="24"
      viewBox="0 0 24 24"
      xmlns="http://www.w3.org/2000/svg"
    >
      <path
        fillRule="evenodd"
        clipRule="evenodd"
        d="M19.85 8.75L24 9.57996V14.42L19.85 15.25L22.2 18.77L18.77 22.2L15.25 19.85L14.42 24H9.57996L8.75 19.85L5.22998 22.2L1.80005 18.77L4.15002 15.25L0 14.42V9.57996L4.15002 8.75L1.80005 5.22998L5.22998 1.80005L8.75 4.15002L9.57996 0H14.42L15.25 4.15002L18.77 1.80005L22.2 5.22998L19.85 8.75ZM18.28 13.8199L22.28 13.01V11.01L18.28 10.2L17.74 8.90002L20.03 5.46997L18.6 4.04004L15.17 6.32996L13.87 5.79004L13.0601 1.79004H11.0601L10.25 5.79004L8.94995 6.32996L5.52002 4.04004L4.08997 5.46997L6.38 8.90002L5.83997 10.2L1.83997 11.01V13.01L5.83997 13.8199L6.38 15.12L4.08997 18.55L5.52002 19.98L8.94995 17.6899L10.25 18.23L11.0601 22.23H13.0601L13.87 18.23L15.17 17.6899L18.6 19.98L20.03 18.55L17.74 15.12L18.28 13.8199ZM10.0943 9.14807C10.6584 8.77118 11.3216 8.56995 12 8.56995C12.9089 8.57258 13.7798 8.93484 14.4225 9.57751C15.0652 10.2202 15.4274 11.0911 15.43 12C15.43 12.6784 15.2288 13.3416 14.8519 13.9056C14.475 14.4697 13.9394 14.9093 13.3126 15.1689C12.6859 15.4286 11.9962 15.4965 11.3308 15.3641C10.6654 15.2318 10.0543 14.9051 9.57457 14.4254C9.09488 13.9457 8.7682 13.3345 8.63585 12.6692C8.50351 12.0038 8.57143 11.3141 8.83104 10.6874C9.09065 10.0606 9.53029 9.52496 10.0943 9.14807ZM11.0499 13.4218C11.3311 13.6097 11.6618 13.71 12 13.71C12.2249 13.7113 12.4479 13.668 12.656 13.5825C12.8641 13.4971 13.0531 13.3712 13.2121 13.2122C13.3712 13.0531 13.497 12.8641 13.5825 12.656C13.668 12.4479 13.7113 12.2249 13.7099 12C13.7099 11.6618 13.6096 11.3311 13.4217 11.0499C13.2338 10.7687 12.9669 10.5496 12.6544 10.4202C12.3419 10.2907 11.9981 10.2569 11.6664 10.3229C11.3347 10.3889 11.03 10.5517 10.7909 10.7909C10.5517 11.03 10.3888 11.3347 10.3229 11.6664C10.2569 11.9981 10.2907 12.342 10.4202 12.6544C10.5496 12.9669 10.7687 13.2339 11.0499 13.4218Z"
      />
    </svg>
  );
}

export function Structure() {
  return (
    <svg
      width="24"
      height="24"
      viewBox="0 0 16 16"
      xmlns="http://www.w3.org/2000/svg"
    >
      <path
        fillRule="evenodd"
        clipRule="evenodd"
        d="M2 2L1 3V6L2 7H14L15 6V3L14 2H2ZM2 3H3H13H14V4V5V6H13H3H2V5V4V3ZM1 10L2 9H5L6 10V13L5 14H2L1 13V10ZM3 10H2V11V12V13H3H4H5V12V11V10H4H3ZM10 10L11 9H14L15 10V13L14 14H11L10 13V10ZM12 10H11V11V12V13H12H13H14V12V11V10H13H12Z"
      />
    </svg>
  );
}

export function Format() {
  return (
    <svg
      width="24"
      height="24"
      viewBox="0 0 16 16"
      xmlns="http://www.w3.org/2000/svg"
    >
      <path
        fillRule="evenodd"
        clipRule="evenodd"
        d="M6 2.98361V2.97184V2H5.91083C5.59743 2 5.29407 2.06161 5.00128 2.18473C4.70818 2.30798 4.44942 2.48474 4.22578 2.71498C4.00311 2.94422 3.83792 3.19498 3.73282 3.46766L3.73233 3.46898C3.63382 3.7352 3.56814 4.01201 3.53533 4.29917L3.53519 4.30053C3.50678 4.5805 3.4987 4.86844 3.51084 5.16428C3.52272 5.45379 3.52866 5.74329 3.52866 6.03279C3.52866 6.23556 3.48974 6.42594 3.412 6.60507L3.4116 6.60601C3.33687 6.78296 3.23423 6.93866 3.10317 7.07359C2.97644 7.20405 2.82466 7.31055 2.64672 7.3925C2.4706 7.46954 2.28497 7.5082 2.08917 7.5082H2V7.6V8.4V8.4918H2.08917C2.28465 8.4918 2.47001 8.53238 2.64601 8.61334L2.64742 8.61396C2.82457 8.69157 2.97577 8.79762 3.10221 8.93161L3.10412 8.93352C3.23428 9.0637 3.33659 9.21871 3.41129 9.39942L3.41201 9.40108C3.48986 9.58047 3.52866 9.76883 3.52866 9.96721C3.52866 10.2567 3.52272 10.5462 3.51084 10.8357C3.4987 11.1316 3.50677 11.4215 3.53516 11.7055L3.53535 11.7072C3.56819 11.9903 3.63387 12.265 3.73232 12.531L3.73283 12.5323C3.83793 12.805 4.00311 13.0558 4.22578 13.285C4.44942 13.5153 4.70818 13.692 5.00128 13.8153C5.29407 13.9384 5.59743 14 5.91083 14H6V13.2V13.0164H5.91083C5.71095 13.0164 5.52346 12.9777 5.34763 12.9008C5.17396 12.8191 5.02194 12.7126 4.89086 12.5818C4.76386 12.4469 4.66104 12.2911 4.58223 12.1137C4.50838 11.9346 4.47134 11.744 4.47134 11.541C4.47134 11.3127 4.4753 11.0885 4.48321 10.8686C4.49125 10.6411 4.49127 10.4195 4.48324 10.2039C4.47914 9.98246 4.46084 9.76883 4.42823 9.56312C4.39513 9.35024 4.33921 9.14757 4.26039 8.95536C4.18091 8.76157 4.07258 8.57746 3.93616 8.40298C3.82345 8.25881 3.68538 8.12462 3.52283 8C3.68538 7.87538 3.82345 7.74119 3.93616 7.59702C4.07258 7.42254 4.18091 7.23843 4.26039 7.04464C4.33913 6.85263 4.39513 6.65175 4.42826 6.44285C4.46082 6.2333 4.47914 6.01973 4.48324 5.80219C4.49127 5.58262 4.49125 5.36105 4.48321 5.13749C4.4753 4.9134 4.47134 4.68725 4.47134 4.45902C4.47134 4.26019 4.50833 4.07152 4.58238 3.89205C4.66135 3.71034 4.76421 3.55475 4.89086 3.42437C5.02193 3.28942 5.17461 3.18275 5.34802 3.10513C5.5238 3.02427 5.71113 2.98361 5.91083 2.98361H6ZM10 13.0164V13.0282V14H10.0892C10.4026 14 10.7059 13.9384 10.9987 13.8153C11.2918 13.692 11.5506 13.5153 11.7742 13.285C11.9969 13.0558 12.1621 12.805 12.2672 12.5323L12.2677 12.531C12.3662 12.2648 12.4319 11.988 12.4647 11.7008L12.4648 11.6995C12.4932 11.4195 12.5013 11.1316 12.4892 10.8357C12.4773 10.5462 12.4713 10.2567 12.4713 9.96721C12.4713 9.76444 12.5103 9.57406 12.588 9.39493L12.5884 9.39399C12.6631 9.21704 12.7658 9.06134 12.8968 8.92642C13.0236 8.79595 13.1753 8.68945 13.3533 8.6075C13.5294 8.53046 13.715 8.4918 13.9108 8.4918H14V8.4V7.6V7.5082H13.9108C13.7153 7.5082 13.53 7.46762 13.354 7.38666L13.3526 7.38604C13.1754 7.30844 13.0242 7.20238 12.8978 7.06839L12.8959 7.06648C12.7657 6.9363 12.6634 6.78129 12.5887 6.60058L12.588 6.59892C12.5101 6.41953 12.4713 6.23117 12.4713 6.03279C12.4713 5.74329 12.4773 5.45379 12.4892 5.16428C12.5013 4.86842 12.4932 4.57848 12.4648 4.29454L12.4646 4.29285C12.4318 4.00971 12.3661 3.73502 12.2677 3.46897L12.2672 3.46766C12.1621 3.19499 11.9969 2.94422 11.7742 2.71498C11.5506 2.48474 11.2918 2.30798 10.9987 2.18473C10.7059 2.06161 10.4026 2 10.0892 2H10V2.8V2.98361H10.0892C10.2891 2.98361 10.4765 3.0223 10.6524 3.09917C10.826 3.18092 10.9781 3.28736 11.1091 3.41823C11.2361 3.55305 11.339 3.70889 11.4178 3.88628C11.4916 4.0654 11.5287 4.25596 11.5287 4.45902C11.5287 4.68727 11.5247 4.91145 11.5168 5.13142C11.5088 5.35894 11.5087 5.58049 11.5168 5.79605C11.5209 6.01754 11.5392 6.23117 11.5718 6.43688C11.6049 6.64976 11.6608 6.85243 11.7396 7.04464C11.8191 7.23843 11.9274 7.42254 12.0638 7.59702C12.1765 7.74119 12.3146 7.87538 12.4772 8C12.3146 8.12462 12.1765 8.25881 12.0638 8.40298C11.9274 8.57746 11.8191 8.76157 11.7396 8.95536C11.6609 9.14737 11.6049 9.34825 11.5717 9.55715C11.5392 9.7667 11.5209 9.98027 11.5168 10.1978C11.5087 10.4174 11.5087 10.6389 11.5168 10.8625C11.5247 11.0866 11.5287 11.3128 11.5287 11.541C11.5287 11.7398 11.4917 11.9285 11.4176 12.1079C11.3386 12.2897 11.2358 12.4452 11.1091 12.5756C10.9781 12.7106 10.8254 12.8173 10.652 12.8949C10.4762 12.9757 10.2889 13.0164 10.0892 13.0164H10Z"
      />
    </svg>
  );
}

export function Token() {
  return (
    <svg
      width="24"
      height="24"
      viewBox="0 0 24 24"
      xmlns="http://www.w3.org/2000/svg"
    >
      <path
        fillRule="evenodd"
        clipRule="evenodd"
        d="M19.5 0V1.5L21 3V22.5L19.5 24H4.5L3 22.5V3L4.5 1.5V0H6V1.5H9V0H10.5V1.5H13.5V0H15V1.5H18V0H19.5ZM4.5 22.5H19.5V3H4.5V22.5ZM7.5 6H16.5V7.5H7.5V6ZM16.5 12H7.5V13.5H16.5V12ZM7.5 18H16.5V19.5H7.5V18Z"
      />
    </svg>
  );
}

export function FormatterIR() {
  return (
    <svg
      width="24"
      height="24"
      viewBox="0 0 16 16"
      xmlns="http://www.w3.org/2000/svg"
    >
      <path
        fillRule="evenodd"
        clipRule="evenodd"
        d="M4 2H12V6C12.3415 6.03511 12.6774 6.11234 13 6.22998V1H3V9.47998L4 7.72998V2ZM6.14001 10L5 8L4 9.75L3.29004 11L1 15H9L6.70996 11L6.14001 10ZM2.71997 14L4.43994 11L5 10L5.56006 11L7.28003 14H2.71997ZM9.55552 7.58984C10.1311 7.20526 10.8077 7 11.5 7C12.4282 7 13.3185 7.36877 13.9748 8.02515C14.6312 8.68152 15 9.57174 15 10.5C15 11.1922 14.7947 11.8689 14.4101 12.4445C14.0256 13.02 13.4789 13.4686 12.8393 13.7335C12.1998 13.9984 11.4961 14.0678 10.8171 13.9327C10.1382 13.7977 9.51461 13.4643 9.02513 12.9749C8.53564 12.4854 8.20229 11.8618 8.06724 11.1829C7.93219 10.5039 8.00155 9.80019 8.26646 9.16064C8.53137 8.5211 8.97995 7.97443 9.55552 7.58984ZM10.1111 12.5786C10.5222 12.8533 11.0055 13 11.5 13C12.163 13 12.799 12.7367 13.2678 12.2678C13.7366 11.799 14 11.163 14 10.5C14 10.0055 13.8533 9.52221 13.5786 9.11108C13.3039 8.69996 12.9135 8.37953 12.4566 8.19031C11.9998 8.00109 11.4973 7.95163 11.0123 8.0481C10.5274 8.14456 10.0818 8.38255 9.73216 8.73218C9.38253 9.08181 9.14454 9.52738 9.04808 10.0123C8.95161 10.4973 9.00107 10.9998 9.19029 11.4567C9.37951 11.9135 9.69994 12.3039 10.1111 12.5786Z"
      />
    </svg>
  );
}

export function Comments() {
  return (
    <svg
      viewBox="0 0 24 24"
      xmlns="http://www.w3.org/2000/svg"
      width={24}
      height={24}
    >
      <path
        d="M7.7,18.3H19.4a2.1,2.1,0,0,0,2.1-2.1V4.6a2.1,2.1,0,0,0-2.1-2.1H4.6A2.1,2.1,0,0,0,2.5,4.6V21.5Z"
        stroke="#ffffff"
        fill="none"
      ></path>
    </svg>
  );
}

export function Add() {
  return (
    <svg
      viewBox="0 0 16 16"
      xmlns="http://www.w3.org/2000/svg"
      className="fill-current"
      width={16}
      height={16}
    >
      <path d="M14.0004 7V8H8.00037V14H7.00037V8H1.00037V7H7.00037V1H8.00037V7H14.0004Z" />
    </svg>
  );
}

export function Close() {
  return (
    <svg
      viewBox="0 0 16 16"
      xmlns="http://www.w3.org/2000/svg"
      className="fill-current"
      width={16}
      height={16}
    >
      <path
        fillRule="evenodd"
        clipRule="evenodd"
        d="M8.00028 8.70711L11.6467 12.3536L12.3538 11.6465L8.70739 8.00001L12.3538 4.35356L11.6467 3.64645L8.00028 7.2929L4.35384 3.64645L3.64673 4.35356L7.29317 8.00001L3.64673 11.6465L4.35384 12.3536L8.00028 8.70711Z"
      />
    </svg>
  );
}

// https://github.com/material-extensions/vscode-material-icon-theme/blob/main/icons/python.svg
export function Python({
  height = 24,
  width = 24,
}: {
  height?: number;
  width?: number;
}) {
  return (
    <svg
      viewBox="0 0 24 24"
      xmlns="http://www.w3.org/2000/svg"
      width={width}
      height={height}
    >
      <path
        fill="#0288D1"
        d="M9.86 2A2.86 2.86 0 0 0 7 4.86v1.68h4.29c.39 0 .71.57.71.96H4.86A2.86 2.86 0 0 0 2 10.36v3.781a2.86 2.86 0 0 0 2.86 2.86h1.18v-2.68a2.85 2.85 0 0 1 2.85-2.86h5.25c1.58 0 2.86-1.271 2.86-2.851V4.86A2.86 2.86 0 0 0 14.14 2zm-.72 1.61c.4 0 .72.12.72.71s-.32.891-.72.891c-.39 0-.71-.3-.71-.89s.32-.711.71-.711z"
      />
      <path
        fill="#fdd835"
        d="M17.959 7v2.68a2.85 2.85 0 0 1-2.85 2.859H9.86A2.85 2.85 0 0 0 7 15.389v3.75a2.86 2.86 0 0 0 2.86 2.86h4.28A2.86 2.86 0 0 0 17 19.14v-1.68h-4.291c-.39 0-.709-.57-.709-.96h7.14A2.86 2.86 0 0 0 22 13.64V9.86A2.86 2.86 0 0 0 19.14 7zM8.32 11.513l-.004.004c.012-.002.025-.001.038-.004zm6.54 7.276c.39 0 .71.3.71.89a.71.71 0 0 1-.71.71c-.4 0-.72-.12-.72-.71s.32-.89.72-.89z"
      />
    </svg>
  );
}

/** https://github.com/material-extensions/vscode-material-icon-theme/blob/main/icons/json.svg */
export function Json({
  height = 24,
  width = 24,
}: {
  height?: number;
  width?: number;
}) {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 -960 960 960"
      width={width}
      height={height}
    >
      <path
        fill="#f9a825"
        d="M560-160v-80h120q17 0 28.5-11.5T720-280v-80q0-38 22-69t58-44v-14q-36-13-58-44t-22-69v-80q0-17-11.5-28.5T680-720H560v-80h120q50 0 85 35t35 85v80q0 17 11.5 28.5T840-560h40v160h-40q-17 0-28.5 11.5T800-360v80q0 50-35 85t-85 35zm-280 0q-50 0-85-35t-35-85v-80q0-17-11.5-28.5T120-400H80v-160h40q17 0 28.5-11.5T160-600v-80q0-50 35-85t85-35h120v80H280q-17 0-28.5 11.5T240-680v80q0 38-22 69t-58 44v14q36 13 58 44t22 69v80q0 17 11.5 28.5T280-240h120v80z"
      />
    </svg>
  );
}

/** https://github.com/material-extensions/vscode-material-icon-theme/blob/main/icons/jupyter.svg **/
export function Jupyter({
  height = 24,
  width = 24,
}: {
  height?: number;
  width?: number;
}) {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 32 32"
      width={width}
      height={height}
    >
      <path
        fill="#f57c00"
        d="M6.2 18a22.7 22.7 0 0 0 9.8 2 22.7 22.7 0 0 0 9.8-2 10.002 10.002 0 0 1-19.6 0m19.6-4a22.7 22.7 0 0 0-9.8-2 22.7 22.7 0 0 0-9.8 2 10.002 10.002 0 0 1 19.6 0"
      />
      <circle cx="27" cy="5" r="3" fill="#757575" />
      <circle cx="5" cy="27" r="3" fill="#9e9e9e" />
      <circle cx="5" cy="5" r="3" fill="#616161" />
    </svg>
  );
}
