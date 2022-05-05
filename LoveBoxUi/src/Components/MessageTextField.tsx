import {
  FormControl,
  Grid,
  IconButton,
  InputLabel,
  MenuItem,
  Select,
  TextField,
  Typography,
} from "@mui/material";
import { Dispatch, FC, SetStateAction, useState } from "react";
import SendIcon from "@mui/icons-material/Send";
import PreviewIcon from "@mui/icons-material/Preview";
import config from "../config";
import { MessageChangeRequest, MessagePreviewRequest } from "../types";

interface IMessageTextField {
  setNoFontSnackBarOpen: Dispatch<SetStateAction<boolean>>;
  setSuccessSnackBarOpen: Dispatch<SetStateAction<boolean>>;
  setErrorSnackBarOpen: Dispatch<SetStateAction<boolean>>;
}

const messageTextFieldGridStyle = {
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  height: "auto",
  backgroundColor: "#3B3B3B",
  width: "95%",
  borderRadius: "10px",
};

const messageTextFieldGridItemStyle = {
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  paddingTop: "10px",
};

const messageTextFieldStyle = {
  width: "95%",
};

const selectFontFormControlStyle = {
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
};

const selectFontInputStyle = {
  width: "95%",
  color: "white",
};

const selectFontGridItemStyle = {
  paddingTop: "20px",
};

const inputLabelStyle = {
  color: "white",
};

const iconStyle = {
  color: "white",
  fontSize: "30px",
};

const messagePreviewGridItemStyle = {
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  paddingTop: "10px",
  paddingBottom: "10px",
};

const iconGridItemStyle = {
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  paddingBottom: "10px",
};

const messagePreviewStyle = {
  color: "white",
  width: "94%",
};

const MessageTextField: FC<IMessageTextField> = (props): JSX.Element => {
  const [message, setMessage] = useState<string>("");
  const [fontsize, setFontSize] = useState<number>(0);
  const [messagePreview, setMessagePreview] = useState<string>("");

  const sendMessageOnClick = () => {
    if (fontsize === 0) {
      props.setNoFontSnackBarOpen(true);
      return;
    }

    const messageChangeRequest: MessageChangeRequest = {
      fontsize: fontsize,
      image: false,
      message: message,
    };

    fetch(config.EINK_CHANGE_MESSAGE_API, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify(messageChangeRequest),
    })
      .catch((error) => {
        console.log("ERROR", error);
        props.setErrorSnackBarOpen(true);
      })
      .finally(() => props.setSuccessSnackBarOpen(true));
  };

  const getMessagePreviewOnClick = () => {
    if (fontsize === 0) {
      props.setNoFontSnackBarOpen(true);
      return;
    }
    console.log(messagePreview);
    const messagePreviewRequest: MessagePreviewRequest = {
      fontsize: fontsize,
      message: message,
    };

    fetch(config.EINK_PREVIEW_MESSAGE_API, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify(messagePreviewRequest),
    })
      .then((response) => response.json())
      .then((data) => setMessagePreview(data.message))
      .catch((error) => {
        console.log("ERROR", error);
        props.setErrorSnackBarOpen(true);
      });
  };

  return (
    <Grid container style={messageTextFieldGridStyle}>
      <Grid style={selectFontGridItemStyle} item xs={12}>
        <FormControl style={selectFontFormControlStyle} fullWidth>
          <InputLabel style={inputLabelStyle}>Font Size</InputLabel>
          <Select
            value={fontsize}
            style={selectFontInputStyle}
            label="Age"
            onChange={(event) => setFontSize(event.target.value as number)}
          >
            <MenuItem value={30}>30</MenuItem>
            <MenuItem value={48}>48</MenuItem>
            <MenuItem value={64}>64</MenuItem>
            <MenuItem value={96}>96</MenuItem>
          </Select>
        </FormControl>
      </Grid>
      <Grid item xs={12} style={messageTextFieldGridItemStyle}>
        <TextField
          style={messageTextFieldStyle}
          variant="outlined"
          id="standard-textarea"
          multiline
          label="Message"
          onChange={(event) => setMessage(event.target.value)}
          InputProps={{
            style: { color: "white", fontSize: "20px" },
          }}
          InputLabelProps={{
            style: { color: "white", fontSize: "16px" },
          }}
        />
      </Grid>
      <Grid item xs={12} style={messagePreviewGridItemStyle}>
        <Typography
          sx={{ display: "inline-block", whiteSpace: "pre-line" }}
          style={messagePreviewStyle}
        >
          {messagePreview}
        </Typography>
      </Grid>
      <Grid item xs={6} style={iconGridItemStyle}>
        <IconButton onClick={getMessagePreviewOnClick}>
          <PreviewIcon style={iconStyle} />
        </IconButton>
      </Grid>
      <Grid item xs={6} style={iconGridItemStyle}>
        <IconButton onClick={sendMessageOnClick}>
          <SendIcon style={iconStyle} />
        </IconButton>
      </Grid>
    </Grid>
  );
};

export default MessageTextField;
